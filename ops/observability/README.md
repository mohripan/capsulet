# Capsulet Observability Assets

This directory contains starter Prometheus and Grafana assets for clusters that
manage observability outside the Capsulet Helm release.

## Prometheus Alerts

Apply the Prometheus Operator rules:

```powershell
kubectl apply -f ops/observability/capsulet-alerts.yaml
```

The rules cover API error rate, admission rejection, queue age, worker lease
age, execution-pool saturation, scheduler lag, workflow critical-path latency,
stuck workflows, retry exhaustion, and trigger runtime failures.

## Grafana Dashboard

If your Grafana deployment watches ConfigMaps with `grafana_dashboard: "1"`,
create a dashboard ConfigMap from the chart asset:

```powershell
kubectl create configmap capsulet-grafana-dashboards `
  --from-file=capsulet-overview.json=ops/observability/capsulet-overview.json `
  --dry-run=client -o yaml | kubectl apply -f -
```

The Helm chart can render the same dashboard ConfigMap with:

```yaml
grafanaDashboards:
  enabled: true
```
