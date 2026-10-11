# The ProjectRem Worker

`worker/` is a Cloudflare Worker named **`projectrem-api`**. It owns everything
the desktop app must not: account credentials, OAuth client secrets and the
stored provider tokens.

Deployed at `https://projectrem-api.<subdomain>.workers.dev`. `push.bat`
deploys it and reports any missing Worker secret.

## Why a Worker at all

| Concern | Where it lives |
| --- | --- |
| Password hashing and session storage | Worker |
| OAuth **client secrets** | Worker secrets (`wrangler secret put`) |
| OAuth **client ids** | Compiled into the app — public by design |
| Authorization code → token exchange | Worker |
| Hugging Face API tokens | Worker, write-only |
| Model files | Hugging Face, downloaded by the app directly |
| Everything else (downloads, UI, audio) | This machine |

The app never holds a client secret. Connecting a service is: open the browser,
catch the redirect on `localhost`, post the authorization code to the Worker, and
let the Worker do the exchange.

## Storage

One KV namespace, bound as `PR`. Keys are namespaced by prefix so a prefix scan
returns exactly one collection.

| Key | Value | Notes |
| --- | --- | --- |
| `user:<lowercased username>` | user JSON | The canonical record. Written once at register. |
| `userid:<user id>` | username | Reverse lookup so a session can find its user in one hop. |
| `session:<token>` | `{ user_id, created_at }` | 30 day TTL, deleted on logout. |
| `conn:<user id>:<provider>` | connection JSON | `provider` is `discord`, `huggingface` or `github`. |
| `token:<user id>:<provider>` | `{ provider, token, created_at }` | e.g. the user's HuggingFace token. |
| `bot:<bot id>` | bot JSON | A bot definition. |
| `bots:<user id>` | `[bot id, ...]` | Which bots a user owns. |

### User record

```jsonc
{
  "id": "hex12",
  "username": "nikla",
  "password_hash": "pbkdf2-sha256 hex, 100k iterations, 32 bytes",
  "salt": "hex16",
  "display_name": "nikla",
  "avatar_url": "https://... or null",
  "created_at": "2026-10-11T12:00:00.000Z"
}
```

`password_hash` and `salt` are never returned by the API.

### Connection record

```jsonc
{
  "provider": "huggingface",
  "external_id": "hf user id",
  "username": "the user on that service",
  "avatar_url": "https://... or null",
  "access_token": "oauth token, never returned",
  "connected_at": "..."
}
```

The first connection that supplies an `avatar_url` also becomes the profile
picture, which is how "your PFP comes from the service you connect" works.
Picking **None** in the app clears it again.

### Tokens

API tokens are write-only. `GET /api/tokens/<provider>` reports whether one is
stored and when, never the value.

## Endpoints

All authenticated routes need `Authorization: Bearer <session token>`.

### Public

| Method | Path | Body / Result |
| --- | --- | --- |
| `GET` | `/api/health` | `{ ok, service, version }` |
| `POST` | `/api/auth/register` | `{ username, password }` → the user. 409 if taken. |
| `POST` | `/api/auth/login` | `{ username, password }` → `{ token, user }`. |
| `POST` | `/api/oauth/exchange` | `{ provider, code, redirect_uri }` → `{ username, avatar_url, access_token }`. Needs a session. |

### Account

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/api/auth/me` | the current user |
| `POST` | `/api/auth/logout` | deletes the session |
| `PATCH` | `/api/profile` | `{ avatar_url }`, or `null` to clear it |

### Connections

| Method | Path | Result |
| --- | --- | --- |
| `GET` | `/api/connections` | every connection for the user |
| `POST` | `/api/connections/<provider>` | stores the OAuth result |
| `DELETE` | `/api/connections/<provider>` | disconnects |

### Tokens and bots

| Method | Path | Result |
| --- | --- | --- |
| `PUT` | `/api/tokens/<provider>` | stores the token; never returns it |
| `GET` | `/api/tokens/<provider>` | `{ stored, created_at }` |
| `GET` | `/api/bots` | the user's bots |
| `POST` | `/api/bots` | creates one |
| `PATCH` | `/api/bots/<id>` | updates one |
| `DELETE` | `/api/bots/<id>` | deletes one |

## OAuth setup

The registered callback URLs are:

```
http://localhost:53630/callback/hf
http://localhost:53630/callback/dc
http://localhost:53630/callback/gh
```

GitHub accepts exactly **one** callback URL per OAuth app, so it has to be
`/callback/gh` — the short form matches the other two providers and is what the
`redirect_uri` mismatch error will tell you if it is wrong.

To (re)set a secret, from `worker/`:

```sh
wrangler secret put HF_CLIENT_ID
wrangler secret put HF_CLIENT_SECRET
wrangler secret put DC_CLIENT_ID
wrangler secret put DC_CLIENT_SECRET
wrangler secret put GH_CLIENT_ID
wrangler secret put GH_CLIENT_SECRET
```

`push.bat` lists any of the six that are missing after a deploy.

## Consistency caveat

KV is eventually consistent. Registering the same username twice within a few
seconds can race and create two records; the second write wins. Everything else
is read-after-write consistent because it targets the same key.

## Why KV and not a SQL database

The token used here is rejected by Turso (`invalid JWT token`) and lacks the
scope D1 and R2 need (`Authentication error`). KV works with it and is a better
fit anyway: the data is small, read-heavy and keyed by opaque strings, so there
is nothing to join and nothing to index.
