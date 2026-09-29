#!/usr/bin/env bash
set -euo pipefail

output="${1:-THIRD_PARTY_LICENSES.html}"
version="0.9.2"
archive="cargo-about-${version}-x86_64-unknown-linux-musl.tar.gz"
url="https://github.com/EmbarkStudios/cargo-about/releases/download/${version}/${archive}"
sha256="9099a59e820c38a68b9d65f300662a567d56562f9a10f6aa4c7e86c17c2566af"

tool_dir=".cishape/tools/cargo-about-${version}"
download=".cishape/tools/${archive}"
binary=".cishape/tools/cargo-about"

mkdir -p ".cishape/tools" "$(dirname "${output}")"

if [ ! -x "${binary}" ]; then
  rm -rf "${tool_dir}"
  mkdir -p "${tool_dir}"

  curl --fail --location --silent --show-error "${url}" --output "${download}"
  echo "${sha256}  ${download}" | sha256sum --check -

  tar -xzf "${download}" -C "${tool_dir}"
  extracted="$(find "${tool_dir}" -type f -name cargo-about -print -quit)"
  if [ -z "${extracted}" ]; then
    echo "cargo-about binary was not found in ${archive}" >&2
    exit 1
  fi

  install -m 0755 "${extracted}" "${binary}"
fi

tmp="${output}.tmp"
"${binary}" generate about.hbs > "${tmp}"

if ! grep -q "CIShape Third-Party Licenses" "${tmp}"; then
  echo "cargo-about output did not contain the expected report heading" >&2
  exit 1
fi

mv "${tmp}" "${output}"
echo "wrote ${output}"
