# Secrets and Rotation

Production deployments should use an external secret manager rather than static plaintext values in Helm values.

## Supported Patterns

- External Secrets Operator syncing from AWS Secrets Manager, Azure Key Vault, Google Secret Manager, or Vault.
- Sealed Secrets for GitOps-managed encrypted Kubernetes Secret manifests.
- Vault Agent or CSI Secret Store for runtime-mounted credentials.

## Rotation Procedure

1. Create the new secret value in the external secret manager.
2. Wait for the Kubernetes Secret sync controller to update the target Secret.
3. Restart API, worker, scheduler, evaluator, and dashboard pods that consume the secret through environment variables.
4. Verify `/readyz` and a write-path API request.
5. Revoke the old secret value after all pods are running with the new value.

Rotate these values at minimum:

- `CAPSULET_DATABASE_URL`
- `CAPSULET_API_TOKENS`
- `CAPSULET_WEBHOOK_SECRETS`
- object storage access keys
- OIDC client credentials when configured
