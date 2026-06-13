# Send Email Example

This example is a Capsulet job definition script for an hourly email automation.

By default it runs in dry-run mode, prints what it would send, and writes an artifact named `email-summary.txt`. That makes it safe for local Docker Compose and minikube smoke tests.

## Environment

- Capsulet input `recipient`: recipient address. Defaults to `mohripan16@gmail.com`.
- Capsulet input `subject`: email subject.
- Capsulet input `body`: plain-text body.
- `CAPSULET_EMAIL_TO`, `CAPSULET_EMAIL_SUBJECT`, `CAPSULET_EMAIL_BODY`: fallback environment variables.
- `CAPSULET_EMAIL_DRY_RUN`: defaults to `true`.
- `SMTP_HOST`: SMTP host, required only when dry-run is disabled.
- `SMTP_PORT`: defaults to `587`.
- `SMTP_USERNAME`: SMTP username.
- `SMTP_PASSWORD`: SMTP password.
- `SMTP_FROM`: sender address. Defaults to `SMTP_USERNAME`, then `capsulet@example.local`.
- `SMTP_TLS`: defaults to `true`.

## Use In The Dashboard

1. Start the local stack with `docker compose up --build`.
2. Open `http://127.0.0.1:3000/job-definitions`.
3. Create a job definition and paste `send_email.py` into the script field.
4. Open `Workflows`, create a workflow that uses the email job definition.
5. Open `Automations`, create an interval automation. For an hourly schedule, set `Interval seconds` to `3600`.
6. Watch workflow runs in `Automations` and underlying job runs in `Runs`.

To send a real email, set `CAPSULET_EMAIL_DRY_RUN=false` and provide SMTP variables through the execution environment before running the worker. Do not commit SMTP credentials.
