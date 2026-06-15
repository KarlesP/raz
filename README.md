# 🦍 raz — a Rust port of a slice of the Azure CLI

```text
        .="=.
      _/.-.-.\_     _
     ( ( o o ) )    ))
      |/  "  \|    //      r a z
       \'---'/    //       the cheeky little ape that apes `az`
       /`"""`\\  ((
      / /_,_\ \\  \\
      \_\\_'__/ \  ))
      /`  /`~\  |//
     /   /    \  /
 ,--`,--'\/\    /
  '-- "--'  '--'
```

`raz` reimplements a slice of the Azure CLI (`az`) in Rust, invoked as `raz`. It mirrors az's
command-module design and ships **two front-ends over one core library**:

- **`raz`** — a minimal CLI (`raz login`, `raz logout`, `raz account …`, `raz vnet …`, `raz vm …`).
- **`raz-tui`** — an interactive [ratatui](https://ratatui.rs) + [tachyonfx](https://github.com/ratatui/tachyonfx)
  dashboard that browses subscriptions, VMs, and VNets with animated view transitions.

## Workspace

```
crates/
  raz-core/   # engine: context, config, auth (device-code), ARM client, output
  raz/        # minimal CLI front-end (clap)
  raz-tui/    # ratatui + tachyonfx dashboard front-end
```

`raz-core` modules map onto how az structures a command module:

| az | raz-core |
|---|---|
| command table (`commands.py`) | clap subcommand tree + `command::Command` trait |
| `load_arguments` / `_params.py` | clap `#[derive(Args/Subcommand)]` |
| `custom.py` | `arm::vm` / `arm::vnet` / `auth` + front-end command fns |
| `_format.py` table transformers | `output::{render, TableSpec}` |
| global `--subscription/--output/--query` | `context::GlobalArgs` |
| `~/.azure` profile | `config::Profile` (`~/.raz/profile.json`) |

## Scope (this skeleton)

- **Live:** `login` (OAuth device-code flow against Entra, with az-style cross-tenant
  subscription discovery), `logout`, `account` (list/show/set/list-tenants), and `vnet`/`vm`
  `list` + `show` (real ARM REST GETs).
- **Stubbed with explanatory errors:** `vnet`/`vm` `create`/`delete` and `vm` `start`/`stop`
  (these are ARM mutations / long-running operations).
- HTTP uses `reqwest`. A production port would back `arm::client` and the token credential
  with `azure_core` (`Pipeline` + `BearerTokenPolicy`) and `azure_identity`; the
  `auth::credential::TokenSource` and `arm::client` seams are shaped for that swap.

## Build & test

The repository root *is* the Cargo workspace:

```bash
cargo build --release
cargo test
cargo clippy --all-targets
```

## Run

```bash
raz login                          # device-code prompt; discovers tenants + subscriptions
raz account list -o table          # all subscriptions across tenants
raz account set -s <id|name>       # set the active subscription (persisted to ~/.raz)
raz account list-tenants           # distinct tenants

raz vm list -o table               # VMs in the active subscription
raz vnet list -o table             # virtual networks
raz vm show -g <rg> -n <name>      # single VM as JSON
raz -s <id|name> vm list           # override subscription for one command
raz --query "0.name" vm list       # minimal dotted-path projection
raz logout                         # clears ~/.raz

raz-tui                            # interactive dashboard (q/Esc to quit)
```

Exit codes follow az: `0` success, `1` generic/auth error, `2` usage, `3` resource not found.

## Versioning & branching

GitFlow with git-driven semantic versioning via the reusable
[`KarlesP/cadence`](https://github.com/KarlesP/cadence) workflow. The root `VERSION` file is
the source of truth; `main`/`release/*`/`hotfix/*` produce tags + GitHub Releases.
See `.github/workflows/`.

## Releases

Two manual (`workflow_dispatch`) workflows build the binaries for Linux and Windows:

- **Build (debug)** — debug binaries uploaded as artifacts.
- **Build (release)** — optimized binaries uploaded as artifacts and attached to a GitHub
  Release tagged from `VERSION`.

## Credits

Created and maintained by [**KarlesP**](https://github.com/KarlesP).
