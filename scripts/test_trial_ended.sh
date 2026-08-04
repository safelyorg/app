#!/bin/bash
# Sends a real, correctly-signed subscription.paid webhook, simulating
# the moment a trial genuinely converts to a real, paid period - status
# "active" this time, with a real amount_paid, matching exactly what
# Creem sends once the trial ends and the first real charge succeeds.
#
# Usage:
# sed -i 's/\r$//' scripts/test_trial_ended.sh
# chmod +x scripts/test_trial_ended.sh

read -p "Your real subscription ID (sub_...): " SUB_ID
read -p "Your real customer ID (cust_...): " CUST_ID
read -p "Your real product ID (prod_...): " PROD_ID
read -p "Plan name (Team or Enterprise): " PLAN_NAME
read -p "Price in cents (20000 for Team, 50000 for Enterprise): " PRICE
read -p "Your real email address: " EMAIL
read -p "Your real Safely user ID (uuid): " USER_ID
read -sp "Your real CREEM_WEBHOOK_SECRET: " SECRET
echo ""

PAYLOAD=$(cat <<JSON
{"eventType":"subscription.paid","object":{"id":"$SUB_ID","object":"subscription","product":{"id":"$PROD_ID","object":"product","name":"$PLAN_NAME","price":$PRICE,"currency":"USD","billing_type":"recurring","billing_period":"every-month","status":"active","tax_mode":"exclusive","tax_category":"saas","mode":"test"},"customer":{"id":"$CUST_ID","object":"customer","email":"$EMAIL","name":"Test User","country":"PK","mode":"test"},"items":[{"object":"subscription_item","id":"sitem_manual_test","product_id":"$PROD_ID","price_id":"pprice_manual_test","units":1,"mode":"test"}],"collection_method":"charge_automatically","status":"active","last_transaction_id":"tran_manual_test_$(date +%s)","last_transaction":{"id":"tran_manual_test_$(date +%s)","object":"transaction","amount":$PRICE,"amount_paid":$PRICE,"currency":"USD","type":"invoice","status":"paid","mode":"test"},"current_period_start_date":"2026-08-11T06:28:58.491Z","current_period_end_date":"2026-09-11T06:28:58.491Z","canceled_at":null,"metadata":{"safely_user_id":"$USER_ID"},"mode":"test"},"id":"evt_manual_test_$(date +%s)","created_at":$(date +%s)}
JSON
)

SIGNATURE=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')

echo ""
echo "Sending real, signed subscription.paid (post-trial, active) event to your local server..."
curl -X POST http://localhost:3000/api/v1/webhooks/creem \
  -H "Content-Type: application/json" \
  -H "creem-signature: $SIGNATURE" \
  -d "$PAYLOAD"
echo ""
