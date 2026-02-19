#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="vpn-switcher"
BIN_NAME="vpn-switcher"
INSTALL_BIN="/usr/local/bin/${BIN_NAME}"
SERVICE_PATH="/etc/systemd/system/${SERVICE_NAME}.service"
ENV_DIR="/etc/vpn-switcher"
ENV_PATH="${ENV_DIR}/vpn-switcher.env"
STATE_DIR="/var/lib/vpn-switcher"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root: sudo ./scripts/install-systemd.sh"
  exit 1
fi

ensure_build_tools() {
  if command -v cc >/dev/null 2>&1; then
    return 0
  fi

  echo "C compiler (cc) not found. Installing build tools..."

  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config
    return 0
  fi

  if command -v dnf >/dev/null 2>&1; then
    dnf install -y gcc gcc-c++ make pkgconf-pkg-config
    return 0
  fi

  if command -v yum >/dev/null 2>&1; then
    yum install -y gcc gcc-c++ make pkgconfig
    return 0
  fi

  if command -v pacman >/dev/null 2>&1; then
    pacman -Sy --noconfirm base-devel pkgconf
    return 0
  fi

  echo "Unsupported package manager. Install a C toolchain manually (cc/gcc/clang + make + pkg-config)."
  exit 1
}

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

cd "${ROOT_DIR}"

echo "[1/7] Building release binary..."
ensure_build_tools
if command -v cargo >/dev/null 2>&1; then
  cargo build --release
elif [[ -n "${SUDO_USER:-}" ]]; then
  USER_HOME="$(getent passwd "${SUDO_USER}" | cut -d: -f6)"
  USER_CARGO="${USER_HOME}/.cargo/bin/cargo"
  if [[ -x "${USER_CARGO}" ]]; then
    echo "cargo not found for root, using ${USER_CARGO}..."
    su - "${SUDO_USER}" -c "cd '${ROOT_DIR}' && '${USER_CARGO}' build --release"
  else
    echo "cargo not found for root, building as ${SUDO_USER}..."
    su - "${SUDO_USER}" -c "cd '${ROOT_DIR}' && cargo build --release"
  fi
else
  echo "cargo not found in PATH. Install Rust/Cargo or run with: sudo -E ./scripts/install-systemd.sh"
  exit 1
fi

echo "[2/7] Installing binary to ${INSTALL_BIN}..."
install -m 0755 "${ROOT_DIR}/target/release/${BIN_NAME}" "${INSTALL_BIN}"

echo "[3/7] Creating config/state directories..."
install -d -m 0755 "${ENV_DIR}" "${STATE_DIR}"

echo "[4/7] Installing systemd unit..."
install -m 0644 "${ROOT_DIR}/deploy/systemd/${SERVICE_NAME}.service" "${SERVICE_PATH}"

echo "[5/7] Installing env config (first install only)..."
if [[ ! -f "${ENV_PATH}" ]]; then
  install -m 0644 "${ROOT_DIR}/config/vpn-switcher.env.example" "${ENV_PATH}"
  echo "Created ${ENV_PATH}. Edit values if needed."
else
  echo "Keeping existing ${ENV_PATH}."
fi

echo "[6/7] Reloading systemd and enabling service..."
systemctl daemon-reload
systemctl enable --now "${SERVICE_NAME}.service"

echo "[7/7] Service status:"
systemctl --no-pager --full status "${SERVICE_NAME}.service" || true

echo

echo "Installed. Useful commands:"
echo "  systemctl restart ${SERVICE_NAME}"
echo "  systemctl status ${SERVICE_NAME}"
echo "  journalctl -u ${SERVICE_NAME} -f"
