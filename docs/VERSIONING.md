# Versioning

## One source of truth

The only file that contains a version number is `VERSION` in the repository
root:

```
0.0.0
```

Nothing else in the repository hard-codes a version. Not `Cargo.toml`, not the
window title, not the About panel, not the argument handed to the Python
sidecar. They all read it back out of `VERSION` at build time.

## The numbering scheme

```
<major>.<minor>.<patch>
```

* `major` is unbounded: 0, 1, 2, …
* `minor` runs from 0 to 10
* `patch` runs from 0 to 10

So the full ladder is:

```
0.0.0  0.0.1  0.0.2  …  0.0.10
0.1.0  0.1.1  …         0.1.10
…
0.10.0 …                0.10.10
1.0.0  …                          (major rolls over here)
```

`build.bat` advances one step on every run:

| current | next |
| --- | --- |
| `0.0.9` | `0.0.10` |
| `0.0.10` | `0.1.0` |
| `0.9.10` | `0.10.0` |
| `0.10.10` | `1.0.0` |

The bump happens **before** the tests and the build, so a failed run still leaves
`VERSION` advanced. That is deliberate: a version that was attempted but did not
ship should not be reused.

## How the number reaches the rest of the project

```
VERSION  ──read by──▶  main/build.rs  ──cargo:rustc-env──▶  PROJECTREM_VERSION
                                                                     │
                                                     env!("PROJECTREM_VERSION")
                                                                     │
                        ┌────────────────────────────────────────────┼──────────────┐
                        ▼                          ▼                   ▼              ▼
                 window title            Settings → About      --version arg    Out\<version>\
                                                              (Python sidecar)
```

`build.rs` also registers `cargo:rerun-if-changed=../VERSION`, so editing the
file forces a rebuild and the new number appears everywhere at once.

The build is therefore always internally consistent: the folder in `Out\` and
the version printed inside the app can never disagree.

## Changing the version by hand

Edit `VERSION` and rebuild. Useful for a release branch, or to skip ahead.

The file must contain exactly three dot-separated numbers and nothing else
(`build.rs` panics during the build otherwise, so a mistake is loud rather than
silent).

## Build outputs

`build.bat` produces:

```
Out/
└── 0.0.1/
    ├── projectrem-gui.exe
    ├── Start-ProjectRem.bat
    ├── sidecar/
    │   └── main.py
    ├── VERSION
    ├── README.md
    └── build-info.txt      version, timestamp, rustc version
```

If the version folder already exists it is overwritten rather than duplicated.

## From version to GitHub release

The same run continues into a release, so a published version always matches a
version that built and passed its tests:

```
build.bat
  │
  ├─ bump VERSION                     0.0.1 → 0.0.2
  ├─ cargo test --workspace           fails → stop, nothing published
  ├─ cargo build --release
  ├─ package into Out\0.0.2\
  ├─ compress to ProjectRem-0.0.2-windows-x64.zip
  ├─ git tag -a 0.0.2                refuses if the tag already exists
  ├─ gh release create 0.0.2         needs GH_TOKEN or GITHUB_TOKEN
  └─ git push origin 0.0.2
```

The tag is named after the version with **no `v` prefix**, matching what
`VERSION` contains, so the tag, the release and the binary always agree.

`--no-release` stops after packaging. `gh` is only needed for this step; without
it (or without a token) the script says so and leaves the local tag in place
rather than failing a build that otherwise succeeded.

## The Worker's version

`worker/wrangler.toml` carries its own `VERSION` var. It is independent of the
app's and is only surfaced by `GET /api/health`, so a client can tell which
backend it is talking to.