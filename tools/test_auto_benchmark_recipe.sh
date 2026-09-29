#!/usr/bin/env bash
# No real launcher or server: fake mise records literal arguments.
set -euo pipefail
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/mise" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$CAPTURE"
EOF
chmod +x "$tmp/mise"
server="\$(touch $tmp/server-executed)"
account="\$(touch $tmp/account-executed)"
CAPTURE="$tmp/args" PATH="$tmp:$PATH" just auto-benchmark "$server" "$account"
test ! -e "$tmp/server-executed" && test ! -e "$tmp/account-executed"
grep -Fx -- "$server" "$tmp/args" >/dev/null
grep -Fx -- "$account" "$tmp/args" >/dev/null
# The generated script must not contain untrusted values even in dry-run output.
! just --dry-run auto-benchmark "$server" "$account" | grep -F -- "$tmp/" >/dev/null
