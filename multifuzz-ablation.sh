
workers=1
if [[ ! $# -eq 0 ]]; then
    workers=$1
fi

cd bench-harness && ./target/release/bench-harness --workers $workers --backend docker bench ./config/multifuzz-ablation.jinja
