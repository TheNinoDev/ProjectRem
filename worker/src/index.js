/**
 * ProjectRem API - Cloudflare Worker
 *
 * Backs the Account tab in the desktop app:
 *   - accounts   (register / login / session)
 *   - connections (Discord / HuggingFace / GitHub OAuth results + chosen PFP)
 *   - api tokens  (HuggingFace token, so users get their own rate limits)
 *   - bots        (bot definitions served to the app)
 *
 * Storage is a single KV namespace. See schema.md for the key layout.
 *
 * Secrets are configured as Worker secrets, never in this file:
 *   HF_CLIENT_ID, HF_CLIENT_SECRET,
 *   DC_CLIENT_ID, DC_CLIENT_SECRET,
 *   GH_CLIENT_ID, GH_CLIENT_SECRET
 */

const PBKDF2_ITERATIONS = 100000;
const SESSION_TTL_SECONDS = 60 * 60 * 24 * 30; // 30 days
const PROVIDERS = ["discord", "huggingface", "github"];

// ---------------------------------------------------------------- utilities

const json = (data, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      "access-control-allow-origin": "*",
      "access-control-allow-headers": "authorization, content-type",
      "access-control-allow-methods": "GET, POST, PATCH, DELETE, OPTIONS",
    },
  });

const fail = (status, message) => json({ ok: false, error: message }, status);

const enc = new TextEncoder();
const dec = new TextDecoder();

const toHex = (bytes) =>
  [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");

const randomHex = (bytes) => {
  const buf = new Uint8Array(bytes);
  crypto.getRandomValues(buf);
  return toHex(buf);
};

const nowIso = () => new Date().toISOString();

/** PBKDF2-SHA256, returns hex. */
async function hashPassword(password, saltHex) {
  const salt = new Uint8Array(
    (saltHex.match(/.{2}/g) ?? []).map((h) => parseInt(h, 16)),
  );
  const key = await crypto.subtle.importKey(
    "raw",
    enc.encode(password),
    "PBKDF2",
    false,
    ["deriveBits"],
  );
  const bits = await crypto.subtle.deriveBits(
    { name: "PBKDF2", salt, iterations: PBKDF2_ITERATIONS, hash: "SHA-256" },
    key,
    256,
  );
  return toHex(bits);
}

/** Length-independent comparison so verification is not obviously timing bound. */
function safeEqual(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

const readJson = async (kv, key) => {
  const raw = await kv.get(key);
  return raw ? JSON.parse(raw) : null;
};

const writeJson = (kv, key, value) => kv.put(key, JSON.stringify(value));

// -------------------------------------------------------------- validation

const USERNAME_RE = /^[a-zA-Z0-9_.-]{3,24}$/;

function validateCredentials(body) {
  const username = String(body?.username ?? "").trim();
  const password = String(body?.password ?? "");

  if (!USERNAME_RE.test(username)) {
    return "Username must be 3-24 characters (letters, digits, _ . -).";
  }
  if (password.length < 8) return "Password must be at least 8 characters.";
  if (password.length > 200) return "Password is too long.";
  return null;
}

// ------------------------------------------------------------------ routes

async function register(request, env) {
  const body = await request.json().catch(() => ({}));
  const problem = validateCredentials(body);
  if (problem) return fail(400, problem);

  const username = body.username.trim();
  const key = `user:${username.toLowerCase()}`;
  if (await env.PR.get(key)) return fail(409, "That username is already taken.");

  const salt = randomHex(16);
  const user = {
    id: randomHex(12),
    username,
    password_hash: await hashPassword(body.password, salt),
    salt,
    display_name: username,
    avatar_url: null,
    created_at: nowIso(),
  };

  await env.PR.put(key, JSON.stringify(user));
  await env.PR.put(`userid:${user.id}`, username);

  return json({ ok: true, user: publicUser(user) }, 201);
}

async function login(request, env) {
  const body = await request.json().catch(() => ({}));
  const username = String(body?.username ?? "").trim();
  const password = String(body?.password ?? "");
  if (!username || !password) return fail(400, "Username and password are required.");

  const user = await readJson(env.PR, `user:${username.toLowerCase()}`);
  // Same message for "no such user" and "wrong password".
  if (!user) return fail(401, "Wrong username or password.");

  const candidate = await hashPassword(password, user.salt);
  if (!safeEqual(candidate, user.password_hash)) {
    return fail(401, "Wrong username or password.");
  }

  const token = randomHex(32);
  await env.PR.put(
    `session:${token}`,
    JSON.stringify({ user_id: user.id, created_at: nowIso() }),
    { expirationTtl: SESSION_TTL_SECONDS },
  );

  return json({ ok: true, token, user: publicUser(user) });
}

async function logout(env, token) {
  if (token) await env.PR.delete(`session:${token}`);
  return json({ ok: true });
}

function publicUser(user) {
  return {
    id: user.id,
    username: user.username,
    display_name: user.display_name ?? user.username,
    avatar_url: user.avatar_url ?? null,
    created_at: user.created_at,
  };
}

/** Resolve the bearer token to a user, or null. */
async function authenticate(request, env) {
  const header = request.headers.get("authorization") ?? "";
  const token = header.startsWith("Bearer ") ? header.slice(7).trim() : "";
  if (!token) return { user: null, token: null };

  const session = await readJson(env.PR, `session:${token}`);
  if (!session) return { user: null, token };

  const username = await env.PR.get(`userid:${session.user_id}`);
  const user = username ? await readJson(env.PR, `user:${username.toLowerCase()}`) : null;
  return { user, token };
}

async function saveConnection(request, env, user, provider) {
  const body = await request.json().catch(() => ({}));
  const connection = {
    provider,
    external_id: body.external_id ?? null,
    username: body.username ?? null,
    avatar_url: body.avatar_url ?? null,
    access_token: body.access_token ?? null,
    connected_at: nowIso(),
  };

  await writeJson(env.PR, `conn:${user.id}:${provider}`, connection);

  // First connection also sets the profile picture, matching "the PFP comes
  // from whichever service you connect".
  if (!user.avatar_url && connection.avatar_url) {
    user.avatar_url = connection.avatar_url;
    await writeJson(env.PR, `user:${user.username.toLowerCase()}`, user);
  }

  return json({ ok: true, connection: publicConnection(connection) });
}

function publicConnection(connection) {
  const { access_token, ...rest } = connection;
  return { ...rest, connected: Boolean(access_token) };
}

async function listConnections(env, user) {
  const out = [];
  for (const provider of PROVIDERS) {
    const connection = await readJson(env.PR, `conn:${user.id}:${provider}`);
    if (connection) out.push(publicConnection(connection));
  }
  return json({ ok: true, connections: out });
}

async function removeConnection(env, user, provider) {
  await env.PR.delete(`conn:${user.id}:${provider}`);
  return json({ ok: true });
}

async function updateProfile(request, env, user) {
  const body = await request.json().catch(() => ({}));
  if ("avatar_url" in body) user.avatar_url = body.avatar_url || null;
  if ("display_name" in body) {
    const name = String(body.display_name ?? "").trim();
    user.display_name = name.slice(0, 48) || user.username;
  }
  await writeJson(env.PR, `user:${user.username.toLowerCase()}`, user);
  return json({ ok: true, user: publicUser(user) });
}

async function storeToken(request, env, user, provider) {
  const body = await request.json().catch(() => ({}));
  const token = String(body?.token ?? "").trim();
  if (!token) return fail(400, "Token must not be empty.");

  await writeJson(env.PR, `token:${user.id}:${provider}`, {
    provider,
    token,
    created_at: nowIso(),
  });
  return json({ ok: true, provider, stored: true });
}

/** Never returns the secret back, only whether one is on file. */
async function tokenStatus(env, user, provider) {
  const record = await readJson(env.PR, `token:${user.id}:${provider}`);
  return json({
    ok: true,
    provider,
    stored: Boolean(record?.token),
    created_at: record?.created_at ?? null,
  });
}

/**
 * Hand the stored Hugging Face token back to its owner, for use as a download
 * `Authorization` header.
 *
 * This is deliberately a separate route from `tokenStatus`: that one stays
 * write-only and safe for any screen, while this one returns a credential and
 * so is only ever called by the signed-in app on the user's own machine. The
 * token is their own and the app already runs as them, but keeping the two
 * apart means a future UI bug that renders token state cannot leak the secret.
 */
async function downloadToken(env, user, provider) {
  if (provider !== "huggingface") {
    return fail(400, "Only huggingface tokens can be used for downloads.");
  }
  const record = await readJson(env.PR, `token:${user.id}:${provider}`);
  if (!record?.token) {
    return fail(404, "No huggingface token is stored.");
  }
  return json({ ok: true, provider, token: record.token });
}

async function listBots(env, user) {
  const bots = [];
  const owned = await readJson(env.PR, `bots:${user.id}`);
  if (owned) {
    for (const id of owned) {
      const bot = await readJson(env.PR, `bot:${id}`);
      if (bot) bots.push(bot);
    }
  }
  return json({ ok: true, bots });
}

async function createBot(request, env, user) {
  const body = await request.json().catch(() => ({}));
  const name = String(body?.name ?? "").trim();
  if (!name) return fail(400, "A bot needs a name.");
  if (name.length > 48) return fail(400, "Bot name is too long.");

  const bot = {
    id: randomHex(10),
    owner: user.username,
    name,
    provider: body.provider ?? "huggingface",
    model: body.model ?? null,
    system_prompt: String(body.system_prompt ?? "").slice(0, 4000),
    temperature: Number.isFinite(body.temperature) ? body.temperature : 0.7,
    created_at: nowIso(),
  };

  await writeJson(env.PR, `bot:${bot.id}`, bot);
  const owned = (await readJson(env.PR, `bots:${user.id}`)) ?? [];
  owned.push(bot.id);
  await writeJson(env.PR, `bots:${user.id}`, owned);

  return json({ ok: true, bot }, 201);
}

// ------------------------------------------------------------------- OAuth

/**
 * Swap an authorization code for an access token and read the profile.
 *
 * This runs in the Worker so the client secrets never have to ship in the
 * desktop app. Callers must be signed in; the connection is stored for them.
 */
async function oauthExchange(request, env, user) {
  const body = await request.json().catch(() => ({}));
  const provider = String(body?.provider ?? "");
  const code = String(body?.code ?? "");
  if (!PROVIDERS.includes(provider)) return fail(400, `Unknown provider "${provider}".`);
  if (!code) return fail(400, "Missing authorization code.");

  const redirectUri = String(body?.redirect_uri ?? "");

  const tokenUrl = {
    huggingface: "https://huggingface.co/oauth/token",
    discord: "https://discord.com/api/oauth2/token",
    github: "https://github.com/login/oauth/access_token",
  }[provider];

  const clientId = {
    huggingface: env.HF_CLIENT_ID,
    discord: env.DC_CLIENT_ID,
    github: env.GH_CLIENT_ID,
  }[provider];

  const clientSecret = {
    huggingface: env.HF_CLIENT_SECRET,
    discord: env.DC_CLIENT_SECRET,
    github: env.GH_CLIENT_SECRET,
  }[provider];

  if (!clientId || !clientSecret) {
    return fail(503, `${provider} sign-in is not configured on the server.`);
  }

  // GitHub wants a form post, the others accept JSON.
  const payload = {
    client_id: clientId,
    client_secret: clientSecret,
    code,
    redirect_uri: redirectUri,
    grant_type: "authorization_code",
  };

  const exchange = await fetch(tokenUrl, {
    method: "POST",
    headers: {
      accept: "application/json",
      "content-type":
        provider === "github" ? "application/x-www-form-urlencoded" : "application/json",
    },
    body: provider === "github" ? new URLSearchParams(payload) : JSON.stringify(payload),
  });

  const tokens = await exchange.json().catch(() => ({}));
  if (!exchange.ok || !tokens.access_token) {
    return fail(
      401,
      tokens.error_description ?? tokens.error ?? "The provider rejected the authorization code.",
    );
  }
  const accessToken = tokens.access_token;

  const profile = await readProfile(provider, accessToken);
  if (!profile) return fail(502, `Could not read the ${provider} profile.`);

  const connection = {
    provider,
    external_id: profile.id ?? null,
    username: profile.username ?? null,
    avatar_url: profile.avatar ?? null,
    access_token: accessToken,
    connected_at: nowIso(),
  };
  await writeJson(env.PR, `conn:${user.id}:${provider}`, connection);

  // The first service that supplies a picture also becomes the profile picture.
  if (!user.avatar_url && connection.avatar_url) {
    user.avatar_url = connection.avatar_url;
    await writeJson(env.PR, `user:${user.username.toLowerCase()}`, user);
  }

  return json({
    ok: true,
    username: connection.username,
    avatar_url: connection.avatar_url,
    // Handed to the app so downloads can use the user's own quota.
    access_token: accessToken,
  });
}

async function readProfile(provider, accessToken) {
  try {
    if (provider === "huggingface") {
      const r = await fetch("https://huggingface.co/oauth/userinfo", {
        headers: { authorization: `Bearer ${accessToken}` },
      });
      if (!r.ok) return null;
      const p = await r.json();
      return {
        id: p.sub ?? null,
        username: p.name ?? p.preferred_username ?? p.sub ?? null,
        avatar: p.picture ?? null,
      };
    }

    if (provider === "discord") {
      const r = await fetch("https://discord.com/api/v10/users/@me", {
        headers: { authorization: `Bearer ${accessToken}` },
      });
      if (!r.ok) return null;
      const p = await r.json();
      const avatar = p.avatar
        ? `https://cdn.discordapp.com/avatars/${p.id}/${p.avatar}.png?size=256`
        : null;
      return { id: p.id, username: p.global_name ?? p.username ?? null, avatar };
    }

    const r = await fetch("https://api.github.com/user", {
      headers: {
        authorization: `Bearer ${accessToken}`,
        accept: "application/vnd.github+json",
        "user-agent": "ProjectRem",
      },
    });
    if (!r.ok) return null;
    const p = await r.json();
    return { id: p.id ?? null, username: p.login ?? null, avatar: p.avatar_url ?? null };
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------ router

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname;
    const method = request.method;

    if (method === "OPTIONS") return json({}, 204);

    if (path === "/" || path === "/api") {
      return json({
        ok: true,
        service: "ProjectRem API",
        endpoints: [
          "POST   /api/auth/register",
          "POST   /api/auth/login",
          "GET    /api/auth/me",
          "POST   /api/auth/logout",
          "POST   /api/oauth/exchange",
          "GET    /api/connections",
          "POST   /api/connections/:provider",
          "DELETE /api/connections/:provider",
          "PATCH  /api/profile",
          "GET    /api/tokens/:provider",
          "PUT    /api/tokens/:provider",
          "GET    /api/bots",
          "POST   /api/bots",
        ],
      });
    }

    if (path === "/api/health") {
      return json({ ok: true, service: "projectrem-api", version: env.VERSION ?? "dev" });
    }

    // Everything past this point may need a session.
    let user = null;
    let token = null;

    const isPublic = path === "/api/auth/register" || path === "/api/auth/login";
    const isProtected = path.startsWith("/api/");

    if (isProtected) {
      ({ user, token } = await authenticate(request, env));
      if (!isPublic && !user) return fail(401, "Not signed in.");
    }

    try {
      if (path === "/api/auth/register" && method === "POST") return register(request, env);
      if (path === "/api/auth/login" && method === "POST") return login(request, env);
      if (path === "/api/auth/me" && method === "GET") return json({ ok: true, user: publicUser(user) });
      if (path === "/api/auth/logout" && method === "POST") return logout(env, token);

      if (path === "/api/connections" && method === "GET") return listConnections(env, user);
      if (path === "/api/profile" && method === "PATCH") return updateProfile(request, env, user);
      if (path === "/api/oauth/exchange" && method === "POST") {
        return oauthExchange(request, env, user);
      }
      if (path === "/api/bots" && method === "GET") return listBots(env, user);
      if (path === "/api/bots" && method === "POST") return createBot(request, env, user);

      const connection = path.match(/^\/api\/connections\/([a-z]+)$/);
      if (connection) {
        const provider = connection[1];
        if (!PROVIDERS.includes(provider)) return fail(400, `Unknown provider "${provider}".`);
        if (method === "DELETE") return removeConnection(env, user, provider);
        if (method === "POST") return saveConnection(request, env, user, provider);
      }

      const tokenRoute = path.match(/^\/api\/tokens\/([a-z]+)$/);
      if (tokenRoute) {
        const provider = tokenRoute[1];
        if (!PROVIDERS.includes(provider)) return fail(400, `Unknown provider "${provider}".`);
        if (method === "PUT") return storeToken(request, env, user, provider);
        if (method === "GET") return tokenStatus(env, user, provider);
      }

      // Checked before the route above, which would otherwise match
      // `/api/tokens/huggingface/download` only partially.
      const downloadRoute = path.match(/^\/api\/tokens\/([a-z]+)\/download$/);
      if (downloadRoute && method === "GET") {
        return downloadToken(env, user, downloadRoute[1]);
      }
    } catch (err) {
      return fail(500, err?.message ?? "Internal error");
    }

    return fail(404, `No route for ${method} ${path}`);
  },
};