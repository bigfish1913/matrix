#!/bin/bash
# Monitor tank project execution

LOG_FILE="/c/Users/bigfish/Projects/github.com/bigfish1913/tank/workspace/.matrix/matrix.log"
TASK_DIR="/c/Users/bigfish/Projects/github.com/bigfish1913/tank/workspace/.matrix/tasks"

echo "=== Tank Project Monitor ==="
echo "Started at: $(date '+%H:%M:%S')"
echo ""

while true; do
    clear
    echo "=== Tank Project Monitor ==="
    echo "Time: $(date '+%H:%M:%S')"
    echo ""

    # Show latest log entries
    if [ -f "$LOG_FILE" ]; then
        echo "--- Latest Log (last 20 lines) ---"
        tail -20 "$LOG_FILE" 2>/dev/null | grep -E "(Progress|Executing|completed|failed|Claude API|WARN|ERROR)" | tail -10
    fi

    echo ""
    echo "--- Task Status ---"

    # Count task status
    if [ -d "$TASK_DIR" ]; then
        completed=$(grep -l '"status": "completed"' "$TASK_DIR"/*.json 2>/dev/null | wc -l)
        failed=$(grep -l '"status": "failed"' "$TASK_DIR"/*.json 2>/dev/null | wc -l)
        in_progress=$(grep -l '"status": "in_progress"' "$TASK_DIR"/*.json 2>/dev/null | wc -l)
        pending=$(grep -l '"status": "pending"' "$TASK_DIR"/*.json 2>/dev/null | wc -l)

        echo "Completed: $completed | In Progress: $in_progress | Pending: $pending | Failed: $failed"

        # Show in-progress tasks
        if [ $in_progress -gt 0 ]; then
            echo ""
            echo "--- Active Tasks ---"
            for task in "$TASK_DIR"/*.json; do
                if grep -q '"status": "in_progress"' "$task" 2>/dev/null; then
                    task_id=$(basename "$task" .json)
                    title=$(grep '"title"' "$task" 2>/dev/null | head -1 | sed 's/.*"title": "\([^"]*\)".*/\1/')
                    echo "$task_id: $title"
                fi
            done
        fi
    fi

    echo ""
    echo "Press Ctrl+C to exit. Refreshing every 30 seconds..."
    sleep 30
done
