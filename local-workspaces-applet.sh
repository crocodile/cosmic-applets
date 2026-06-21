#!/usr/bin/env bash

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
desktop_id="com.system76.CosmicAppletWorkspaces.desktop"
system_desktop="/usr/share/applications/${desktop_id}"
local_desktop="${XDG_DATA_HOME:-${HOME}/.local/share}/applications/${desktop_id}"
binary="${script_dir}/target/release/cosmic-applet-workspaces"

usage() {
    cat <<EOF
Usage: $(basename "$0") [install|build|status|uninstall]

  install    Build and activate the local applet (default)
  build      Rebuild the applet without changing the desktop entry
  status     Show which applet executable the desktop entry uses
  uninstall  Remove the local desktop-entry override
EOF
}

build() {
    echo "Building the workspace applet..."
    cargo build \
        --manifest-path "${script_dir}/Cargo.toml" \
        --release \
        -p cosmic-applet-workspaces
}

install_override() {
    if [[ ! -f "${system_desktop}" ]]; then
        echo "System desktop entry not found: ${system_desktop}" >&2
        exit 1
    fi

    build
    mkdir -p "$(dirname -- "${local_desktop}")"
    cp -- "${system_desktop}" "${local_desktop}"
    sed -i "s|^Exec=.*|Exec=${binary}|" "${local_desktop}"

    echo
    echo "Local applet activated:"
    echo "  ${binary}"
    echo
    echo "In COSMIC panel settings, remove Numbered Workspaces and add it again."
    echo "If COSMIC still uses the packaged applet, log out and back in once."
}

show_status() {
    if [[ -f "${local_desktop}" ]]; then
        echo "Local override: ${local_desktop}"
        grep '^Exec=' "${local_desktop}" || true
    else
        echo "No local override is active."
        echo "COSMIC will use: ${system_desktop}"
        grep '^Exec=' "${system_desktop}" || true
    fi

    if [[ -x "${binary}" ]]; then
        echo "Built binary: ${binary}"
    else
        echo "Built binary is missing."
    fi
}

uninstall_override() {
    rm -f -- "${local_desktop}"
    echo "Removed local override: ${local_desktop}"
    echo "Remove and re-add Numbered Workspaces to restore the packaged applet."
}

case "${1:-install}" in
    install)
        install_override
        ;;
    build)
        build
        ;;
    status)
        show_status
        ;;
    uninstall)
        uninstall_override
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
