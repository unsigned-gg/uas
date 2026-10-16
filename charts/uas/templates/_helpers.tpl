{{- define "uas.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "uas.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- .Chart.Name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "uas.labels" -}}
helm.sh/chart: {{ .Chart.Name }}-{{ .Chart.Version | replace "+" "_" }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: uas
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end -}}

{{- define "uas.operatorImage" -}}
{{- if .Values.operator.image.digest -}}
{{ .Values.operator.image.repository }}:{{ .Values.operator.image.tag }}@{{ .Values.operator.image.digest }}
{{- else -}}
{{ .Values.operator.image.repository }}:{{ .Values.operator.image.tag }}
{{- end -}}
{{- end -}}

{{- define "uas.daemonImage" -}}
{{- if .Values.daemon.image.digest -}}
{{ .Values.daemon.image.repository }}:{{ .Values.daemon.image.tag }}@{{ .Values.daemon.image.digest }}
{{- else -}}
{{ .Values.daemon.image.repository }}:{{ .Values.daemon.image.tag }}
{{- end -}}
{{- end -}}
