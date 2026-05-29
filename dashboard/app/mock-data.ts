export const automations = [
  {
    name: "Nightly report",
    target: "generate-report",
    pool: "mini",
    status: "enabled",
    trigger: "0 2 * * *",
    condition: "nightly",
    lastRun: "Succeeded 18m ago",
    success: 99.1,
    owner: "ops",
    events: 1420
  },
  {
    name: "Image resize hook",
    target: "resize-image",
    pool: "mini",
    status: "enabled",
    trigger: "Webhook",
    condition: "upload_event",
    lastRun: "Running now",
    success: 96.4,
    owner: "media",
    events: 884
  },
  {
    name: "Model training approval",
    target: "train-model",
    pool: "large",
    status: "paused",
    trigger: "(data_ready AND approved) OR manual",
    condition: "(data_ready AND approved) OR manual_override",
    lastRun: "Failed 2h ago",
    success: 88.7,
    owner: "ml-platform",
    events: 93
  },
  {
    name: "CSV cleanup",
    target: "normalize-csv",
    pool: "mini",
    status: "enabled",
    trigger: "Manual",
    condition: "manual_submit",
    lastRun: "Queued 4m ago",
    success: 97.2,
    owner: "data",
    events: 211
  }
];

export const workflows = [
  {
    name: "train-model",
    version: "v7",
    status: "active",
    steps: ["prepare-training-data", "train", "evaluate", "publish"],
    pool: "large",
    lastRun: "Failed 2h ago",
    success: 88.7
  },
  {
    name: "daily-report",
    version: "v3",
    status: "active",
    steps: ["extract", "transform", "render-report", "send-email"],
    pool: "mini",
    lastRun: "Succeeded 18m ago",
    success: 99.1
  },
  {
    name: "image-pipeline",
    version: "v2",
    status: "active",
    steps: ["download", "resize", "optimize", "store-artifact"],
    pool: "mini",
    lastRun: "Running now",
    success: 96.4
  }
];

export const runs = [
  {
    id: "run_9ac41",
    automation: "Image resize hook",
    pool: "mini",
    state: "running",
    duration: "01:42",
    node: "mini-a3",
    attempt: "1/3",
    created: "10:42:19"
  },
  {
    id: "run_9ab88",
    automation: "Nightly report",
    pool: "mini",
    state: "succeeded",
    duration: "02:18",
    node: "mini-a1",
    attempt: "1/3",
    created: "10:18:03"
  },
  {
    id: "run_9aa07",
    automation: "Model training approval",
    pool: "large",
    state: "failed",
    duration: "18:44",
    node: "large-b2",
    attempt: "3/3",
    created: "08:31:10"
  },
  {
    id: "run_9a9fd",
    automation: "CSV cleanup",
    pool: "mini",
    state: "queued",
    duration: "-",
    node: "-",
    attempt: "0/2",
    created: "10:47:01"
  },
  {
    id: "run_9a8bc",
    automation: "Nightly report",
    pool: "mini",
    state: "timed_out",
    duration: "10:00",
    node: "mini-a4",
    attempt: "2/2",
    created: "09:22:45"
  }
];

export const pools = [
  {
    name: "mini",
    label: "Small jobs",
    nodes: 4,
    running: 18,
    queued: 7,
    cpu: 58,
    memory: 64,
    timeout: "120s",
    concurrency: 50,
    selector: "capsulet.dev/pool=mini",
    accent: "cyan"
  },
  {
    name: "large",
    label: "Compute jobs",
    nodes: 3,
    running: 5,
    queued: 3,
    cpu: 72,
    memory: 81,
    timeout: "3600s",
    concurrency: 10,
    selector: "capsulet.dev/pool=large",
    accent: "blue"
  },
  {
    name: "gpu",
    label: "Inference",
    nodes: 1,
    running: 1,
    queued: 0,
    cpu: 44,
    memory: 52,
    timeout: "1800s",
    concurrency: 2,
    selector: "capsulet.dev/pool=gpu",
    accent: "violet"
  }
];

export const artifacts = [
  {
    name: "nightly-report-2026-05-29.pdf",
    run: "run_9ab88",
    type: "application/pdf",
    size: "3.8 MB",
    retention: "30 days",
    bucket: "capsulet-artifacts"
  },
  {
    name: "image-resize-output.zip",
    run: "run_9ac41",
    type: "application/zip",
    size: "18.4 MB",
    retention: "30 days",
    bucket: "capsulet-artifacts"
  },
  {
    name: "training-metrics.json",
    run: "run_9aa07",
    type: "application/json",
    size: "92 KB",
    retention: "30 days",
    bucket: "capsulet-artifacts"
  }
];

export const timeline = [
  ["10:42:17", "trigger.webhook.accepted", "resize-uploaded-image"],
  ["10:42:18", "automation.condition.satisfied", "upload_event"],
  ["10:42:19", "job.run.created", "run_9ac41"],
  ["10:42:21", "pool.resolved", "mini"],
  ["10:42:24", "kubernetes.job.created", "capsulet-run-9ac41"],
  ["10:42:31", "artifact.upload.started", "input-bundle.tar.gz"]
];
