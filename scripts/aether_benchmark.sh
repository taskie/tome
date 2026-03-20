#!/bin/bash
set -eu
set -o pipefail

SCRIPT="$(realpath "$0")"

usage() {
    cat <<'EOF'
Usage:
  aether_benchmark.sh cipher <label>       Benchmark cipher x chunk_kind 0,4,7,13
  aether_benchmark.sh chunk-kind  <label>  Benchmark chunk_kind 0–13

Environment variables:
  AETHER_BENCH_DATA   Input file to encrypt    (default: data)
  AETHER_BENCH_KEY    32-byte binary key file  (default: key)
  AETHER_BENCH_PW     Password for pw mode     (default: test)
EOF
    exit 1
}

# Export so hyperfine subprocesses inherit them.
export AETHER="${AETHER:-aether}"
export AETHER_BENCH_DATA="${AETHER_BENCH_DATA:-data}"
export AETHER_BENCH_KEY="${AETHER_BENCH_KEY:-key}"
export AETHER_BENCH_PW="${AETHER_BENCH_PW:-test}"

# Internal: single encryption run
run() {
    local cipher="$1" key_mode="$2" chunk_kind="${3:-7}" jobs="${4:-1}"
    if [[ "$key_mode" = pw ]]; then
        "$AETHER" -P AETHER_BENCH_PW "$AETHER_BENCH_DATA" \
            --cipher "$cipher" --chunk-kind "$chunk_kind" --jobs "$jobs"
    else
        "$AETHER" -k "$AETHER_BENCH_KEY" "$AETHER_BENCH_DATA" \
            --cipher "$cipher" --chunk-kind "$chunk_kind" --jobs "$jobs"
    fi
}

mode="${1:-}"
shift || true

if [ -z "$mode" ] && [ $# -eq 0 ]; then
    mode=cipher
fi

case "$mode" in
    run)
        run "$@"
        ;;

    cipher)
        label="${1:-default}"

        hyperfine --warmup 2 --prepare sync \
            --parameter-list cipher aes256gcm,chacha20,xchacha20 \
            --parameter-list key_mode key \
            --parameter-list chunk_kind 0,4,7,13 \
            --command-name '{cipher} {key_mode} {chunk_kind}' \
            --export-json "cipher.${label}.json" \
            --export-markdown "cipher.${label}.md" \
            "${SCRIPT} run {cipher} {key_mode} {chunk_kind}"
        ;;

    chunk-kind)
        label="${1:-default}"

        hyperfine --warmup 2 --prepare sync \
            --parameter-list chunk_kind 0,1,2,3,4,5,6,7,8,9,10,11,12,13 \
            --command-name 'xchacha20 chunk_kind={chunk_kind}' \
            --export-json "chunk-kind.${label}.json" \
            --export-markdown "chunk-kind.${label}.md" \
            "${SCRIPT} run xchacha20 key {chunk_kind}"
        ;;

    jobs)
        label="${1:-default}"

        hyperfine --warmup 2 --prepare sync \
            --parameter-list jobs 1,2,4,6,8 \
            --parameter-list chunk_kind 4,7,13 \
            --command-name 'xchacha20 jobs={jobs} chunk_kind={chunk_kind}' \
            --export-json "jobs.${label}.json" \
            --export-markdown "jobs.${label}.md" \
            "${SCRIPT} run xchacha20 key {chunk_kind} {jobs}"
        ;;

    *)
        usage
        ;;
esac
