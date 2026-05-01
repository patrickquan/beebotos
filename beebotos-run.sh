#!/usr/bin/env bash
# BeeBotOS Production Runner (Linux/macOS)
# Usage: ./beebotos-run.sh [start|stop|restart|status] [gateway|web|beehub|all]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Parse arguments
ACTION="start"
TARGET="all"
WORKING_DIR=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --working-dir)
            WORKING_DIR="$2"
            shift 2
            ;;
        start|stop|restart|status)
            ACTION="$1"
            shift
            ;;
        gateway|web|beehub|all)
            TARGET="$1"
            shift
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

if [[ -n "$WORKING_DIR" ]]; then
    SCRIPT_DIR="$WORKING_DIR"
fi
cd "${SCRIPT_DIR}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Ensure data directories exist
mkdir -p data/run data/workspace data/logs

SERVICES=(
    "gateway|beebotos-gateway|8000|API Gateway"
    "web|web-server|8090|Web Frontend Server"
    "beehub|beehub|8080|BeeHub Service"
)

get_service_field() {
    local svc="$1"
    local idx="$2"
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name binary port desc <<< "$entry"
        if [[ "$name" == "$svc" ]]; then
            case $idx in
                1) echo "$binary" ;;
                2) echo "$port" ;;
                3) echo "$desc" ;;
            esac
            return
        fi
    done
}

get_pid_file() {
    echo "data/run/${1}.pid"
}

is_running() {
    local svc="$1"
    local pid_file=$(get_pid_file "$svc")
    if [[ -f "$pid_file" ]]; then
        local pid=$(cat "$pid_file")
        if kill -0 "$pid" 2>/dev/null; then
            return 0
        fi
    fi
    # 回退：检查端口是否被监听（兼容 PID 文件丢失的情况）
    local port=$(get_service_field "$svc" 2)
    if command -v netstat &>/dev/null; then
        netstat -an 2>/dev/null | grep "LISTEN" | grep -qE "([:.])$port([[:space:]]|$)" && return 0
    fi
    return 1
}

start_service() {
    local svc="$1"
    local binary=$(get_service_field "$svc" 1)
    local port=$(get_service_field "$svc" 2)
    local desc=$(get_service_field "$svc" 3)
    local pid_file=$(get_pid_file "$svc")

    local binary_path="./${binary}"
    if [[ ! -f "$binary_path" ]]; then
        # 开发环境回退：检查 target/release/
        local dev_binary="target/release/${binary}"
        if [[ -f "$dev_binary" ]]; then
            binary_path="$dev_binary"
        elif [[ "$svc" == "beehub" ]]; then
            echo -e "${YELLOW}BeeHub binary not found, skipping.${NC}"
            return 0
        else
            echo -e "${RED}Binary not found: ./${binary}${NC}"
            return 1
        fi
    fi

    if is_running "$svc"; then
        echo -e "${YELLOW}${desc} is already running (PID: $(cat "$pid_file"))${NC}"
        return 0
    fi

    echo -e "${CYAN}Starting ${desc} on port ${port}...${NC}"
    local log_file="data/logs/${svc}.log"

    if [[ "$svc" == "web" ]]; then
        if [[ -f "./index.html" && -d "./pkg" ]]; then
            # 生产环境：直接使用当前目录
            nohup "$binary_path" --static-path . --gateway-url http://localhost:8000 > "${log_file}" 2>&1 &
        elif [[ -f "apps/web/index.html" ]]; then
            # 开发环境：准备临时静态目录
            local temp_static_dir="${SCRIPT_DIR}/data/temp-web-static"
            rm -rf "$temp_static_dir"
            mkdir -p "$temp_static_dir"
            cp "apps/web/index.html" "$temp_static_dir/"
            cp -r "apps/web/pkg" "$temp_static_dir/"
            cp -r "apps/web/style" "$temp_static_dir/"
            cp "apps/web/style/main.css" "$temp_static_dir/style.css"
            cp "apps/web/style/components.css" "$temp_static_dir/components.css"
            if [[ -f "apps/web/public/favicon.svg" ]]; then
                cp "apps/web/public/favicon.svg" "$temp_static_dir/favicon.svg"
            fi
            if [[ -f "apps/web/public/marked.min.js" ]]; then
                cp "apps/web/public/marked.min.js" "$temp_static_dir/marked.min.js"
            fi
            nohup "$binary_path" --static-path "$temp_static_dir" --gateway-url http://localhost:8000 > "${log_file}" 2>&1 &
        else
            echo -e "${RED}Web static files not found. Please build web first: ./beebotos-dev.sh build web${NC}"
            return 1
        fi
    else
        nohup "$binary_path" > "${log_file}" 2>&1 &
    fi

    local pid=$!
    echo $pid > "$pid_file"

    sleep 1
    if kill -0 "$pid" 2>/dev/null; then
        echo -e "${GREEN}${desc} started (PID: ${pid})${NC}"
    else
        echo -e "${RED}${desc} failed to start. Check ${log_file}${NC}"
        rm -f "$pid_file"
        return 1
    fi
}

stop_service() {
    local svc="$1"
    local desc=$(get_service_field "$svc" 3)
    local pid_file=$(get_pid_file "$svc")

    if ! is_running "$svc"; then
        echo -e "${YELLOW}${desc} is not running${NC}"
        rm -f "$pid_file"
        return 0
    fi

    local pid=$(cat "$pid_file")
    echo -e "${CYAN}Stopping ${desc} (PID: ${pid})...${NC}"

    if kill "$pid" 2>/dev/null; then
        local count=0
        while kill -0 "$pid" 2>/dev/null && [[ $count -lt 10 ]]; do
            sleep 0.5
            count=$((count + 1))
        done
        if kill -0 "$pid" 2>/dev/null; then
            kill -9 "$pid" 2>/dev/null || true
            echo -e "${YELLOW}${desc} force stopped${NC}"
        else
            echo -e "${GREEN}${desc} stopped${NC}"
        fi
    fi
    rm -f "$pid_file"
}

restart_service() {
    local svc="$1"
    stop_service "$svc"
    sleep 1
    start_service "$svc"
}

show_status() {
    echo -e "${CYAN}Service Status${NC}"
    echo -e "${CYAN}----------------------------------------${NC}"
    printf "%-12s %-10s %-8s %s\n" "Service" "Status" "PID" "Port"
    echo "----------------------------------------"
    for entry in "${SERVICES[@]}"; do
        IFS='|' read -r name _ port desc <<< "$entry"
        local pid_file=$(get_pid_file "$name")
        if is_running "$name"; then
            local pid=$(cat "$pid_file")
            printf "%-12s ${GREEN}%-10s${NC} %-8s %s\n" "$name" "running" "$pid" "$port"
        else
            printf "%-12s ${RED}%-10s${NC} %-8s %s\n" "$name" "stopped" "-" "$port"
        fi
    done
}

case "$ACTION" in
    start)
        case "$TARGET" in
            gateway) start_service "gateway" ;;
            web) start_service "web" ;;
            beehub) start_service "beehub" ;;
            all)
                for entry in "${SERVICES[@]}"; do
                    IFS='|' read -r name _ _ _ <<< "$entry"
                    start_service "$name" || true
                done
                ;;
            *)
                echo -e "${RED}Usage: $0 start [gateway|web|beehub|all]${NC}"
                exit 1
                ;;
        esac
        ;;
    stop)
        case "$TARGET" in
            gateway) stop_service "gateway" ;;
            web) stop_service "web" ;;
            beehub) stop_service "beehub" ;;
            all)
                for entry in "${SERVICES[@]}"; do
                    IFS='|' read -r name _ _ _ <<< "$entry"
                    stop_service "$name"
                done
                ;;
            *)
                echo -e "${RED}Usage: $0 stop [gateway|web|beehub|all]${NC}"
                exit 1
                ;;
        esac
        ;;
    restart)
        case "$TARGET" in
            gateway) restart_service "gateway" ;;
            web) restart_service "web" ;;
            beehub) restart_service "beehub" ;;
            all)
                for entry in "${SERVICES[@]}"; do
                    IFS='|' read -r name _ _ _ <<< "$entry"
                    restart_service "$name" || true
                done
                ;;
            *)
                echo -e "${RED}Usage: $0 restart [gateway|web|beehub|all]${NC}"
                exit 1
                ;;
        esac
        ;;
    status)
        show_status
        ;;
    *)
        echo -e "${RED}Usage: $0 [start|stop|restart|status] [gateway|web|beehub|all]${NC}"
        exit 1
        ;;
esac
