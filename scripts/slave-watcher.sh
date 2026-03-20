#!/usr/bin/env bash
# Interclaude Slave Watcher
# Monitors Slave/Inbox for new command files, executes via claude -p,
# writes responses to Slave/Outbox, updates .ledger for dedup.
#
# Usage: ./slave-watcher.sh [INTERCLAUDE_DIR]
#   INTERCLAUDE_DIR defaults to ~/Interclaude

set -euo pipefail

INTERCLAUDE_DIR="${1:-$HOME/Interclaude}"
INBOX="$INTERCLAUDE_DIR/Slave/Inbox"
OUTBOX="$INTERCLAUDE_DIR/Slave/Outbox"
STATUS_DIR="$INTERCLAUDE_DIR/.status"
LEDGER="$INTERCLAUDE_DIR/.ledger"

# Ensure directories exist
mkdir -p "$INBOX" "$OUTBOX" "$STATUS_DIR"
touch "$LEDGER"

echo "[$(date)] Slave watcher started"
echo "[$(date)] Watching: $INBOX"
echo "[$(date)] Outbox:   $OUTBOX"

# Track processed files to avoid re-processing
declare -A PROCESSED

while true; do
    for f in "$INBOX"/*.json; do
        [ -f "$f" ] || continue

        BASENAME=$(basename "$f")

        # Skip already processed in this session
        [[ -n "${PROCESSED[$BASENAME]:-}" ]] && continue

        # Extract msg_id
        MSG_ID=$(python3 -c "import json; print(json.load(open('$f'))['msg_id'])" 2>/dev/null || echo "")

        # Check ledger for dedup
        if [ -n "$MSG_ID" ] && grep -qF "$MSG_ID" "$LEDGER" 2>/dev/null; then
            PROCESSED[$BASENAME]=1
            continue
        fi

        echo "[$(date)] Processing: $BASENAME (msg_id: $MSG_ID)"

        # Update status: READ
        if [ -n "$MSG_ID" ]; then
            python3 -c "
import json, datetime
status = [{'msg_id': '$MSG_ID', 'status': 'READ', 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z'}]
json.dump(status, open('$STATUS_DIR/$MSG_ID.json', 'w'), indent=2)
" 2>/dev/null || true
        fi

        # Extract task
        TASK=$(python3 -c "import json; print(json.load(open('$f'))['payload']['task'])" 2>/dev/null || echo "")
        SEQ=$(python3 -c "import json; print(json.load(open('$f'))['sequence'])" 2>/dev/null || echo "0")

        if [ -z "$TASK" ]; then
            echo "[$(date)] ERROR: Could not parse task from $BASENAME"
            PROCESSED[$BASENAME]=1
            continue
        fi

        # Update status: EXECUTING
        if [ -n "$MSG_ID" ]; then
            python3 -c "
import json, datetime
status = json.load(open('$STATUS_DIR/$MSG_ID.json'))
status.append({'msg_id': '$MSG_ID', 'status': 'EXECUTING', 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z'})
json.dump(status, open('$STATUS_DIR/$MSG_ID.json', 'w'), indent=2)
" 2>/dev/null || true
        fi

        # Execute via claude -p
        START_TIME=$(date +%s%N)
        RESPONSE=$(claude -p "$TASK" 2>&1) || true
        EXIT_CODE=$?
        END_TIME=$(date +%s%N)
        DURATION_MS=$(( (END_TIME - START_TIME) / 1000000 ))

        echo "[$(date)] Executed in ${DURATION_MS}ms (exit: $EXIT_CODE)"

        # Update status: EXECUTED
        if [ -n "$MSG_ID" ]; then
            python3 -c "
import json, datetime
status = json.load(open('$STATUS_DIR/$MSG_ID.json'))
status.append({'msg_id': '$MSG_ID', 'status': 'EXECUTED', 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z'})
json.dump(status, open('$STATUS_DIR/$MSG_ID.json', 'w'), indent=2)
" 2>/dev/null || true
        fi

        # Write response to outbox
        TIMESTAMP=$(date +%Y%m%d_%H%M%S)
        RESP_FILE="$OUTBOX/${TIMESTAMP}_$(printf '%04d' "$SEQ")_response.json"

        python3 -c "
import json, datetime, uuid
resp = {
    'msg_id': str(uuid.uuid4()),
    'msg_type': 'response',
    'timestamp': datetime.datetime.utcnow().isoformat() + 'Z',
    'sequence': int('$SEQ'),
    'sender_role': 'slave',
    'transport_used': 'rsync',
    'payload': {
        'reply_to': '$MSG_ID',
        'stdout': open('/dev/stdin').read(),
        'stderr': '',
        'exit_code': $EXIT_CODE,
        'files_modified': [],
        'duration_ms': $DURATION_MS
    }
}
json.dump(resp, open('$RESP_FILE', 'w'), indent=2)
" <<< "$RESPONSE"

        echo "[$(date)] Response written to $(basename "$RESP_FILE")"

        # Update status: REPLYING
        if [ -n "$MSG_ID" ]; then
            python3 -c "
import json, datetime
status = json.load(open('$STATUS_DIR/$MSG_ID.json'))
status.append({'msg_id': '$MSG_ID', 'status': 'REPLYING', 'timestamp': datetime.datetime.utcnow().isoformat() + 'Z'})
json.dump(status, open('$STATUS_DIR/$MSG_ID.json', 'w'), indent=2)
" 2>/dev/null || true
        fi

        # Mark in ledger
        [ -n "$MSG_ID" ] && echo "$MSG_ID" >> "$LEDGER"

        PROCESSED[$BASENAME]=1
    done

    sleep 2
done
