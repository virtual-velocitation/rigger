#!/bin/sh
# A fixture standing in for the real `podman` CLI, for the container runtime snippet's tests in
# `tests/principle_gates_wiring.rs`: appends its argv, one line per call, to
# $RECORDING_PODMAN_LOG, and answers `ps` with one line per `id:age-in-seconds` entry of
# $RECORDING_PODMAN_CONTAINERS, shaped as podman prints `{{.ID}} {{.CreatedAt}}`, so a test sees
# exactly which containers the snippet listed and removed without any container runtime.
echo "$*" >> "$RECORDING_PODMAN_LOG"
test "$1" = ps || exit 0
now="$(date +%s)"
for entry in $RECORDING_PODMAN_CONTAINERS; do
    echo "${entry%%:*} $(date -d "@$((now - ${entry#*:}))" '+%Y-%m-%d %H:%M:%S.%N %z %Z')"
done
