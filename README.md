# dyt — doneyet CLI

The official command-line client for the **doneyet** workflow / job tracker.
`dyt` wraps the doneyet REST API so producers (CI jobs, Kubernetes `Job`s, cron
scripts) and operators never have to hand-assemble `curl` invocations,
`Authorization` headers, or JSON bodies.

> Crate: `doneyet-cli` &nbsp;·&nbsp; Binary: `dyt`

`dyt` is a thin, stateless HTTP client. It ships as a single self-contained
binary with no runtime dependencies — download it, point it at your doneyet
server, and run it anywhere.

---

## Install

### Option A — Download a prebuilt binary (recommended)

Prebuilt binaries are attached to every [GitHub
Release](https://github.com/lucheeseng827/doneyet-cli/releases). Pick the asset
matching your OS / CPU. Replace `v0.1.0` with the version you want.

#### macOS

```bash
VERSION=v0.1.0

# Detect CPU: arm64 (Apple Silicon) or x86_64 (Intel). Fail fast on anything
# else so we don't construct an empty download URL silently.
case "$(uname -m)" in
  arm64)  TARGET=aarch64-apple-darwin ;;
  x86_64) TARGET=x86_64-apple-darwin ;;
  *)      echo "Unsupported macOS CPU architecture: $(uname -m)" >&2; exit 1 ;;
esac

curl -L "https://github.com/lucheeseng827/doneyet-cli/releases/download/${VERSION}/dyt-${VERSION}-${TARGET}.tar.gz" \
  | tar xz

sudo install -m 0755 "dyt-${VERSION}-${TARGET}/dyt" /usr/local/bin/dyt

# Gatekeeper: the binary is unsigned, so macOS quarantines it on first run.
# Strip the quarantine attribute to allow execution:
sudo xattr -d com.apple.quarantine /usr/local/bin/dyt 2>/dev/null || true

dyt --version
```

#### Linux (x86_64)

```bash
VERSION=v0.1.0
curl -L "https://github.com/lucheeseng827/doneyet-cli/releases/download/${VERSION}/dyt-${VERSION}-x86_64-unknown-linux-gnu.tar.gz" \
  | tar xz
sudo install -m 0755 "dyt-${VERSION}-x86_64-unknown-linux-gnu/dyt" /usr/local/bin/dyt
dyt --version
```

#### Windows (PowerShell)

```powershell
$Version = "v0.1.0"
$Target  = "x86_64-pc-windows-msvc"
$Url     = "https://github.com/lucheeseng827/doneyet-cli/releases/download/$Version/dyt-$Version-$Target.zip"

$Dest = "$env:LOCALAPPDATA\Programs\dyt"
$Tmp  = "$env:TEMP\dyt.zip"
Invoke-WebRequest -Uri $Url -OutFile $Tmp
Expand-Archive -Path $Tmp -DestinationPath $Dest -Force

# Add to PATH for the current user (idempotent, persistent across new shells).
$Bin = (Resolve-Path "$Dest\dyt-$Version-$Target").Path
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
$Parts = if ($UserPath) { $UserPath.Split(";") } else { @() }
if ($Parts -notcontains $Bin) {
  $NewPath = if ($UserPath) { "$UserPath;$Bin" } else { $Bin }
  [Environment]::SetEnvironmentVariable("Path", $NewPath, "User")
}

# Open a new PowerShell and run:  dyt --version
```

### Option B — Install from source with Cargo

Requires a [Rust toolchain](https://rustup.rs) (stable).

```bash
# Straight from the repository:
cargo install --git https://github.com/lucheeseng827/doneyet-cli

# …or from a local checkout:
git clone https://github.com/lucheeseng827/doneyet-cli
cargo install --path doneyet-cli
# → installs `dyt` into ~/.cargo/bin
```

### Option C — Build a release binary manually

```bash
git clone https://github.com/lucheeseng827/doneyet-cli
cd doneyet-cli
cargo build --release
install -m 0755 target/release/dyt /usr/local/bin/dyt
```

---

## Setup & configure

`dyt` resolves configuration in priority order:

1. CLI flags (`--api-url`, `--admin-token`, `--run-token`, `--read-basic`)
2. Environment (`DONEYET_API_URL`, `DONEYET_ADMIN_TOKEN`, `DONEYET_RUN_TOKEN`,
   `DONEYET_READ_BASIC`, `DONEYET_HANDOFF_TOKEN`)
3. `~/.config/doneyet/config.toml` (written `chmod 0600` on Unix)

Quickstart:

```bash
# Point dyt at your doneyet server (defaults to http://localhost:3001)
dyt config set api-url https://doneyet.example.com

dyt login        # prompts for the admin token (hidden input)
dyt whoami       # prints resolved config + pings /health
```

---

## Common workflows

```bash
# Operator — register a workflow
dyt workflow upsert \
  --slug nightly-export \
  --name "Nightly Export" \
  --owner group:default/data \
  --expected-duration-s 3600 \
  --heartbeat-grace-s 120

dyt workflow list
dyt workflow get nightly-export

# Producer — start a run, then heartbeat until done
START=$(dyt run start nightly-export --output json \
        --metadata '{"sha":"abc1234"}' --token-out /tmp/dyt.tok)
RUN_ID=$(echo "$START" | jq -r .id)
export DONEYET_RUN_TOKEN=$(cat /tmp/dyt.tok)

dyt run heartbeat "$RUN_ID" --progress 25 --message "extract"
dyt run tail      "$RUN_ID" --interval 10 --message "still working"   # ^C to stop
dyt run finish    "$RUN_ID" --status succeeded --message "10k rows"

# Operator — observability
dyt overview
dyt run list --status running
dyt sla at-risk --within 30m
dyt handoff list --status offered

# Notifications
dyt notify channel create --kind slack --name oncall \
  --target https://hooks.slack.com/services/XXX
dyt notify create --channel-name oncall --event-type manual \
  --severity warning --title "deploy stalled"
```

---

## Output formats

Every command supports `--output table` (default, human-readable) and
`--output json` (pretty-printed; pipeable into `jq`).

```bash
dyt run list --output json | jq '.[].id'
```

## Shell completion

```bash
dyt completion bash > /etc/bash_completion.d/dyt
dyt completion zsh  > "${fpath[1]}/_dyt"
dyt completion fish > ~/.config/fish/completions/dyt.fish
```

## Auth model — quick reference

| Tier            | Tells dyt via            | Used by                                                                 |
|-----------------|--------------------------|-------------------------------------------------------------------------|
| Admin           | `--admin-token` / config | `workflow upsert/delete`, `run start/cancel`, `handoff offer/expire`, `notify create/update`, `notify channel create/delete` |
| Run token       | `--run-token` / env      | `run heartbeat/tail/finish`, `step create/update`, `run get`            |
| Handoff token   | `--handoff-token` / env  | `run start --continue-from`                                             |
| Read (optional) | `--read-basic user:pass` | `workflow list/get`, `run list`, `handoff list`, `overview`, `sla at-risk`, `notify list/get`, `notify channel list` |

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
