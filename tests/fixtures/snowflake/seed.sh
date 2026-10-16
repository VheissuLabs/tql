#!/usr/bin/env bash
set -euo pipefail

settings="${TQL_SNOWFLAKE_ENV:-$HOME/.config/tql/snowflake-test.env}"
if [ -f "$settings" ]; then
    source "$settings"
fi

account="${TQL_TEST_SNOWFLAKE:?set TQL_TEST_SNOWFLAKE to your account, such as myorg-myaccount, or put it in $settings}"
token="${TQL_TEST_SNOWFLAKE_TOKEN:?set TQL_TEST_SNOWFLAKE_TOKEN to a programmatic access token, or put it in $settings}"
warehouse="${TQL_TEST_SNOWFLAKE_WAREHOUSE:-COMPUTE_WH}"
role="${TQL_TEST_SNOWFLAKE_ROLE:-SYSADMIN}"
here="$(cd "$(dirname "$0")" && pwd)"

case "$account" in
    http://* | https://*) url="${account%/}" ;;
    *.snowflakecomputing.*) url="https://${account%/}" ;;
    *) url="https://${account//_/-}.snowflakecomputing.com" ;;
esac

body=$(jq -n --rawfile statement "$here/seed.sql" --arg warehouse "$warehouse" --arg role "$role" '{
    statement: $statement,
    timeout: 300,
    warehouse: $warehouse,
    role: $role,
    parameters: {MULTI_STATEMENT_COUNT: "0"}
}')

answer=$(curl -sS -w '\n%{http_code}' -X POST "$url/api/v2/statements" \
    -H "Authorization: Bearer $token" \
    -H 'Content-Type: application/json' \
    -H 'Accept: application/json' \
    -d "$body")
status=$(tail -n1 <<<"$answer")
reply=$(sed '$d' <<<"$answer")

while [ "$status" = 202 ]; do
    sleep 1
    answer=$(curl -sS -w '\n%{http_code}' "$url$(jq -r .statementStatusUrl <<<"$reply")" \
        -H "Authorization: Bearer $token" -H 'Accept: application/json')
    status=$(tail -n1 <<<"$answer")
    reply=$(sed '$d' <<<"$answer")
done

if [ "$status" != 200 ]; then
    echo "Seeding failed ($status): $(jq -r '.message // .' <<<"$reply")" >&2
    exit 1
fi
echo "Seeded TQL_TEST at $url"
