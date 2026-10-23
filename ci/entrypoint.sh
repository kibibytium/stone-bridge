#!/usr/bin/env bash
set -e

/stonebridge/server &
PID1=$!

cd /stonebridge/tuta-bridge && node app.js &
PID2=$!

# Forward termination signals to both children
trap 'kill -TERM $PID1 $PID2 2>/dev/null' TERM INT

# Exit as soon as either process exits, propagating its exit code
wait -n $PID1 $PID2
EXIT_CODE=$?

kill -TERM $PID1 $PID2 2>/dev/null
wait $PID1 $PID2 2>/dev/null

exit $EXIT_CODE
