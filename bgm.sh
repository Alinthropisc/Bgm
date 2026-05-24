#!/usr/bin/env bash
# bgm.sh — the project's command hub. Run everything through here so nothing
# leaks into your shell's environment: a project-local `.bgm.env` (if present at
# the repo root) is loaded *only* for the spawned process, never exported to
# your interactive shell.
#
# Usage:
#   ./scripts/bgm.sh <url|flags...>        run a load test (default action)
#   ./scripts/bgm.sh run <url|flags...>    explicit run
#   ./scripts/bgm.sh example <name>        run examples/<name>.yml
#   ./scripts/bgm.sh examples              list bundled examples
#   ./scripts/bgm.sh build                 build the release binary
#   ./scripts/bgm.sh fmt                   cargo fmt (whole workspace)
#   ./scripts/bgm.sh clippy                cargo clippy (whole workspace)
#   ./scripts/bgm.sh test                  cargo test (whole workspace)
#   ./scripts/bgm.sh check                 fmt --check + clippy + test
#   ./scripts/bgm.sh env                   show the project-local environment
#   ./scripts/bgm.sh clean                 cargo clean
#   ./scripts/bgm.sh help                  this help
#
# Environment:
#   BGM_PROFILE   cargo profile: release (default) or debug.
#   .bgm.env      optional KEY=value file at repo root, auto-loaded per-run.

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd -- "${script_dir}/.." && pwd)"
cd "${root_dir}"

# --- project-local environment (kept out of your interactive shell) ---------
load_env() {
  local env_file="${root_dir}/.bgm.env"
  if [[ -f "${env_file}" ]]; then
    set -a            # auto-export following assignments...
    # shellcheck disable=SC1090
    source "${env_file}"
    set +a            # ...only for this process tree.
  fi
}

profile="${BGM_PROFILE:-release}"
case "${profile}" in
  release) cargo_flags=(--release); bin="${root_dir}/target/release/bgm" ;;
  debug)   cargo_flags=();          bin="${root_dir}/target/debug/bgm"   ;;
  *) echo "bgm.sh: unknown BGM_PROFILE '${profile}' (use release|debug)" >&2; exit 2 ;;
esac

log() { echo "bgm.sh: $*" >&2; }

build() {
  log "building (${profile})..."
  cargo build "${cargo_flags[@]}" --bin bgm
}

# Build only when the binary is missing or older than any source file.
ensure_built() {
  if [[ ! -x "${bin}" ]] || [[ -n "$(find "${root_dir}" -name '*.rs' -newer "${bin}" \
        -not -path '*/target/*' -not -path '*/old_projects/*' -print -quit)" ]]; then
    build
  fi
}

run_bin() {
  ensure_built
  load_env
  exec "${bin}" "$@"
}

usage() { sed -n '2,28p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

cmd="${1:-run}"
case "${cmd}" in
  build) build ;;
  run) shift; run_bin "$@" ;;
  example)
    [[ $# -ge 2 ]] || { log "usage: bgm.sh example <name>"; exit 2; }
    run_bin --file "examples/${2}.yml" "${@:3}"
    ;;
  examples)
    log "available examples:"
    for f in "${root_dir}"/examples/*.yml; do echo "  $(basename "${f}" .yml)"; done
    ;;
  fmt)    cargo fmt --all ;;
  clippy) cargo clippy --workspace --all-targets ;;
  test)   cargo test --workspace ;;
  check)  cargo fmt --all --check && cargo clippy --workspace --all-targets && cargo test --workspace ;;
  env)
    load_env
    if [[ -f "${root_dir}/.bgm.env" ]]; then log "project env (.bgm.env):"; cat "${root_dir}/.bgm.env"; else log "no .bgm.env present"; fi
    ;;
  clean) cargo clean ;;
  help|-h|--help) usage ;;
  # Anything else (a URL, a flag) is treated as run arguments.
  *) run_bin "$@" ;;
esac
