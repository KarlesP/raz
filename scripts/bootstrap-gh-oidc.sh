#!/usr/bin/env bash
# Bootstrap GitHub Actions -> Azure auth for the benchmark workflows, using raz itself.
#
# Creates an Entra app registration + service principal (Contributor on the subscription) and a
# GitHub-OIDC federated credential (secretless — no client secret is stored), then writes the
# AZURE_* values as repo secrets so `azure/login` and `raz login --service-principal` can sign in.
#
# Prereqs:
#   * `raz login` already done as an identity with Owner / User-Access-Admin on the target sub.
#   * `gh` authenticated to the repo (gh auth login).
# Usage:  REPO=KarlesP/raz ./scripts/bootstrap-gh-oidc.sh
set -euo pipefail

REPO="${REPO:-KarlesP/raz}"
APP_NAME="${APP_NAME:-raz-bench-oidc}"
# workflow_dispatch runs on the default branch ref, so the OIDC subject is the main-branch ref.
SUBJECT="${SUBJECT:-repo:${REPO}:ref:refs/heads/main}"

command -v raz >/dev/null || { echo "raz not found on PATH"; exit 1; }
command -v gh  >/dev/null || { echo "gh not found on PATH";  exit 1; }
command -v jq  >/dev/null || { echo "jq not found on PATH";  exit 1; }

# -o json + strip quotes so this works on any raz build (older binaries render scalar tsv blank).
SUB=$(raz account show --query id -o json | tr -d '"')
TENANT=$(raz account show --query tenant_id -o json | tr -d '"')
[ -n "$SUB" ] && [ -n "$TENANT" ] || { echo "could not resolve subscription/tenant — run 'raz login'"; exit 1; }

echo "Creating app + service principal '$APP_NAME' with Contributor on /subscriptions/$SUB ..."
SP_JSON=$(raz ad sp create-for-rbac -n "$APP_NAME" --role Contributor --scope "/subscriptions/$SUB" -o json)
APP_ID=$(echo "$SP_JSON" | jq -r '.appId')
[ -n "$APP_ID" ] && [ "$APP_ID" != "null" ] || { echo "failed to create service principal"; echo "$SP_JSON"; exit 1; }

echo "Adding GitHub-OIDC federated credential (subject: $SUBJECT) ..."
raz ad app federated-credential create \
  --id "$APP_ID" -n gh-dispatch \
  --issuer https://token.actions.githubusercontent.com \
  --subject "$SUBJECT" \
  --audience api://AzureADTokenExchange

echo "Writing repo secrets to $REPO ..."
gh secret set AZURE_CLIENT_ID       --repo "$REPO" --body "$APP_ID"
gh secret set AZURE_TENANT_ID       --repo "$REPO" --body "$TENANT"
gh secret set AZURE_SUBSCRIPTION_ID --repo "$REPO" --body "$SUB"

cat <<EOF

Done.
  AZURE_CLIENT_ID        = $APP_ID
  AZURE_TENANT_ID        = $TENANT
  AZURE_SUBSCRIPTION_ID  = $SUB

The client secret raz minted is NOT used or stored (OIDC is secretless) — discard it.
Role assignment + federated credential can take ~1 minute to propagate before the first run.
Trigger a benchmark with:  gh workflow run benchmark-template.yml --repo $REPO
EOF
