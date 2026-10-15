#!/bin/bash
# Look at new reviews and decide which ones show on the website.
# Works for your laptop's database and for the live (production) one.
#
# Run it on your laptop (WSL), from the app folder:
#   bash scripts/reviews.sh              <- new reviews on your laptop
#   bash scripts/reviews.sh prod         <- new reviews on the live website
#   bash scripts/reviews.sh shown        <- reviews already on your laptop's site
#   bash scripts/reviews.sh prod shown   <- reviews already on the live website
#
# Each review is shown one by one with its photo, and you answer:
#   new reviews:    y = show it on the website   n = don't show   s = decide later
#   shown reviews:  k = keep it on the website   h = hide it
#
# Production is reached through the "safely" SSH shortcut in ~/.ssh/config,
# so the production password never leaves the server.

APP_DIR="${SAFELY_APP_DIR:-/mnt/c/Users/Bilal Khan/Documents/projects/Rust/app}"
SERVER="safely"

TARGET="local"
MODE="new"
for arg in "$@"; do
    case "$arg" in
        prod) TARGET="prod" ;;
        local) TARGET="local" ;;
        shown) MODE="shown" ;;
        new) MODE="new" ;;
        *)
            echo "Unknown option: $arg"
            echo "Use: bash scripts/reviews.sh [prod] [shown]"
            exit 1
            ;;
    esac
done

# Running on the server itself: use this server's database.
if [ -f "$HOME/app/backend/.env" ] && [ ! -d "/mnt/c" ]; then
    TARGET="server"
fi

if [ "$TARGET" = "local" ]; then
    if [ ! -f "$APP_DIR/backend/.env" ]; then
        echo "No backend/.env found in $APP_DIR"
        exit 1
    fi
    source <(grep -E '^APP_URL=' "$APP_DIR/backend/.env" | tr -d '\r')
elif [ "$TARGET" = "server" ]; then
    source <(grep -E '^APP_URL=' "$HOME/app/backend/.env" | tr -d '\r')
fi

# Runs SQL (given on stdin) on the chosen database and prints plain rows.
run_sql() {
    if [ "$TARGET" = "prod" ]; then
        ssh "$SERVER" "cd ~/app/backend && source <(grep -E '^APP_URL=' .env | tr -d '\r') && psql \"\$APP_URL\" -X -q -t -A -v ON_ERROR_STOP=1"
    else
        psql "$APP_URL" -X -q -t -A -v ON_ERROR_STOP=1
    fi
}

# Saves a review's photo to a temp file and opens it. Prints the file path.
open_photo() {
    local id="$1"
    local type ext file
    type=$(run_sql <<< "SELECT photo_content_type FROM reviews WHERE id = $id AND photo_data IS NOT NULL;")
    case "$type" in
        image/jpeg) ext=jpg ;;
        image/png) ext=png ;;
        image/webp) ext=webp ;;
        *)
            echo "   (no photo)"
            return
            ;;
    esac
    file="/tmp/safely-review-$id.$ext"
    run_sql <<< "SELECT encode(photo_data, 'base64') FROM reviews WHERE id = $id;" | base64 -d > "$file"

    if command -v explorer.exe > /dev/null 2>&1; then
        explorer.exe "$(wslpath -w "$file")" > /dev/null 2>&1
        echo "   Photo opened in a new window."
    elif command -v xdg-open > /dev/null 2>&1 && [ -n "$DISPLAY" ]; then
        xdg-open "$file" > /dev/null 2>&1 &
        echo "   Photo opened in a new window."
    else
        echo "   Photo saved at $file (photos can't open on the server - run this from your laptop to see them)."
    fi
}

# Prints one review in a readable way.
show_review() {
    run_sql <<< "SELECT
        '   Name:    ' || display_name || coalesce('  ·  ' || role, '') || E'\n' ||
        '   Stars:   ' || repeat('★', rating) || repeat('☆', 5 - rating) || E'\n' ||
        '   Review:  ' || body || E'\n' ||
        '   Written: ' || to_char(updated_at, 'DD Mon YYYY')
        FROM reviews WHERE id = $1;"
}

case "$TARGET" in
    prod) WHERE_LABEL="the LIVE website (production)" ;;
    server) WHERE_LABEL="this server's website" ;;
    *) WHERE_LABEL="your laptop's website" ;;
esac

if [ "$MODE" = "new" ]; then
    IDS=$(run_sql <<< "SELECT id FROM reviews WHERE status = 'pending' ORDER BY updated_at;") || {
        echo "Could not read reviews. Check the database (and 'ssh safely' for prod)."
        exit 1
    }
else
    IDS=$(run_sql <<< "SELECT id FROM reviews WHERE status = 'approved' ORDER BY approved_at DESC;") || {
        echo "Could not read reviews. Check the database (and 'ssh safely' for prod)."
        exit 1
    }
fi

read -r -a ID_LIST <<< "$(echo $IDS)"
TOTAL=${#ID_LIST[@]}

echo
if [ "$TOTAL" -eq 0 ]; then
    if [ "$MODE" = "new" ]; then
        echo "No new reviews on $WHERE_LABEL."
    else
        echo "No reviews are shown on $WHERE_LABEL yet."
    fi
    exit 0
fi

if [ "$MODE" = "new" ]; then
    echo "$TOTAL new review(s) on $WHERE_LABEL."
else
    echo "$TOTAL review(s) shown on $WHERE_LABEL."
fi

COUNT=0
for ID in "${ID_LIST[@]}"; do
    COUNT=$((COUNT + 1))
    echo
    echo "-------- Review $COUNT of $TOTAL --------"
    show_review "$ID"
    open_photo "$ID"
    echo

    while true; do
        if [ "$MODE" = "new" ]; then
            read -r -p "   Show it on the website? (y = yes, n = no, s = decide later): " ANSWER < /dev/tty
            case "$ANSWER" in
                y | Y)
                    DONE=$(run_sql <<< "UPDATE reviews SET status = 'approved', approved_at = NOW(), updated_at = NOW() WHERE id = $ID AND photo_data IS NOT NULL RETURNING id;")
                    if [ -n "$DONE" ]; then
                        echo "   ✓ Live on the website."
                    else
                        echo "   Not shown: this review has no photo yet."
                    fi
                    break
                    ;;
                n | N)
                    run_sql <<< "UPDATE reviews SET status = 'rejected', approved_at = NULL, updated_at = NOW() WHERE id = $ID;" > /dev/null
                    echo "   ✗ Won't be shown."
                    break
                    ;;
                s | S)
                    echo "   Skipped. It will be asked again next time."
                    break
                    ;;
            esac
        else
            read -r -p "   Keep it on the website? (k = keep, h = hide): " ANSWER < /dev/tty
            case "$ANSWER" in
                k | K)
                    echo "   Kept."
                    break
                    ;;
                h | H)
                    run_sql <<< "UPDATE reviews SET status = 'rejected', approved_at = NULL, updated_at = NOW() WHERE id = $ID;" > /dev/null
                    echo "   Hidden from the website."
                    break
                    ;;
            esac
        fi
    done
    rm -f /tmp/safely-review-"$ID".* 2> /dev/null
done

echo
echo "Done."
