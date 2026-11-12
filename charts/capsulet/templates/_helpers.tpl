{{/*
Expand the chart name.
*/}}
{{- define "capsulet.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Bundled PostgreSQL resource name.
*/}}
{{- define "capsulet.postgresqlName" -}}
{{- printf "%s-postgresql" (include "capsulet.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
Bundled SeaweedFS resource name.
*/}}
{{- define "capsulet.seaweedfsName" -}}
{{- printf "%s-seaweedfs" (include "capsulet.fullname" .) | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{/*
A third-party image from its values: repository:tag, with @digest when one is
given, so the tag says what it is and the digest decides what runs.
*/}}
{{- define "capsulet.dependencyImage" -}}
{{- if .digest -}}
{{- printf "%s:%s@%s" .repository .tag .digest -}}
{{- else -}}
{{- printf "%s:%s" .repository .tag -}}
{{- end -}}
{{- end -}}

{{/*
Database Secret name used by API, worker, and migration job.
*/}}
{{- define "capsulet.databaseSecretName" -}}
{{- if eq .Values.postgresql.mode "bundled" -}}
{{- include "capsulet.postgresqlName" . -}}
{{- else -}}
{{- required "config.databaseUrlSecret.name is required when postgresql.mode=external" .Values.config.databaseUrlSecret.name -}}
{{- end -}}
{{- end -}}

{{/*
Database Secret key used by API, worker, and migration job.
*/}}
{{- define "capsulet.databaseSecretKey" -}}
{{- if eq .Values.postgresql.mode "bundled" -}}
DATABASE_URL
{{- else -}}
{{- default "DATABASE_URL" .Values.config.databaseUrlSecret.key -}}
{{- end -}}
{{- end -}}

{{/*
Effective object storage mode.
*/}}
{{- define "capsulet.objectStorageMode" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
s3
{{- else -}}
{{- .Values.config.objectStorage.mode -}}
{{- end -}}
{{- end -}}

{{/*
Effective object storage bucket.
*/}}
{{- define "capsulet.objectStorageBucket" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
{{- .Values.seaweedfs.bucket -}}
{{- else -}}
{{- .Values.config.objectStorage.bucket -}}
{{- end -}}
{{- end -}}

{{/*
Effective object storage endpoint.
*/}}
{{- define "capsulet.objectStorageEndpoint" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
{{- printf "http://%s:%v" (include "capsulet.seaweedfsName" .) .Values.seaweedfs.service.port -}}
{{- else -}}
{{- .Values.config.objectStorage.endpoint -}}
{{- end -}}
{{- end -}}

{{/*
Effective object storage region.
*/}}
{{- define "capsulet.objectStorageRegion" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
{{- .Values.seaweedfs.region -}}
{{- else -}}
{{- .Values.config.objectStorage.region -}}
{{- end -}}
{{- end -}}

{{/*
Effective object storage path style setting.
*/}}
{{- define "capsulet.objectStoragePathStyle" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
true
{{- else -}}
{{- .Values.config.objectStorage.pathStyle -}}
{{- end -}}
{{- end -}}

{{/*
Object storage credentials Secret name used by the API, worker and evaluator.
*/}}
{{- define "capsulet.objectStorageCredentialsSecretName" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
{{- include "capsulet.seaweedfsName" . -}}
{{- else if eq (include "capsulet.objectStorageMode" .) "s3" -}}
{{- required "config.objectStorage.credentialsSecret.name is required when using external S3-compatible object storage" .Values.config.objectStorage.credentialsSecret.name -}}
{{- end -}}
{{- end -}}

{{/*
Object storage access key Secret key.
*/}}
{{- define "capsulet.objectStorageAccessKeyKey" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
access-key-id
{{- else -}}
{{- default "access-key-id" .Values.config.objectStorage.credentialsSecret.accessKeyKey -}}
{{- end -}}
{{- end -}}

{{/*
Object storage secret key Secret key.
*/}}
{{- define "capsulet.objectStorageSecretKeyKey" -}}
{{- if eq .Values.seaweedfs.mode "bundled" -}}
secret-access-key
{{- else -}}
{{- default "secret-access-key" .Values.config.objectStorage.credentialsSecret.secretKeyKey -}}
{{- end -}}
{{- end -}}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "capsulet.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{/*
Common labels.
*/}}
{{- define "capsulet.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
app.kubernetes.io/name: {{ include "capsulet.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{/*
Selector labels.
*/}}
{{- define "capsulet.selectorLabels" -}}
app.kubernetes.io/name: {{ include "capsulet.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/*
Service account name.
*/}}
{{- define "capsulet.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "capsulet.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{/* 
Component image.
*/}}
{{- define "capsulet.componentImage" -}}
{{- $root := index . 0 -}}
{{- $component := index . 1 -}}
{{- $path := list $root.Values.image.registry $root.Values.image.namespace $component.image.name | compact | join "/" -}}
{{- $reference := printf "%s:%s" $path $root.Values.image.tag -}}
{{- with $component.image.digest -}}
{{- $reference = printf "%s@%s" $reference . -}}
{{- end -}}
{{- $reference -}}
{{- end -}}

{{/*
CEL expression for execution image allowlists. Patterns ending in * are treated
as prefixes, matching the runner's pool policy behavior.
*/}}
{{- define "capsulet.allowedImagesCel" -}}
{{- $checks := list -}}
{{- range $image := . -}}
{{- if hasSuffix "*" $image -}}
{{- $prefix := trimSuffix "*" $image -}}
{{- $checks = append $checks (printf "c.image.startsWith(%q)" $prefix) -}}
{{- else -}}
{{- $checks = append $checks (printf "c.image == %q" $image) -}}
{{- end -}}
{{- end -}}
{{- printf "object.spec.containers.all(c, %s)" (join " || " $checks) -}}
{{- end -}}

{{/*
The port a background component serves health and metrics on.
*/}}
{{- define "capsulet.healthPort" -}}
{{- $ports := dict "worker" 8081 "scheduler" 8082 "evaluator" 8083 "graph-worker" 8084 -}}
{{- get $ports . -}}
{{- end }}
