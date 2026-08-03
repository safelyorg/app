#!/bin/bash
# Sends a real, correctly-signed subscription.past_due webhook directly
# to your own local server - using YOUR real email, so you can
# actually see the rendered email arrive, without depending on Creem's
# generic test-event tool (which always uses a fake sample email).
#
# Usage:
# SELECT creem_subscription_id, creem_customer_id, creem_product_id FROM subscriptions ORDER BY created_at DESC LIMIT 1;
# SELECT id FROM users ORDER BY created_at DESC LIMIT 1;
# ./test_past_due.sh

read -p "Your real subscription ID (sub_...): " SUB_ID
read -p "Your real customer ID (cust_...): " CUST_ID
read -p "Your real product ID (prod_...): " PROD_ID
read -p "Your real email address: " EMAIL
read -p "Your real Safely user ID (uuid): " USER_ID
read -sp "Your real CREEM_WEBHOOK_SECRET: " SECRET
echo ""

PAYLOAD=$(cat <<JSON
{"eventType":"subscription.past_due","object":{"id":"$SUB_ID","object":"subscription","product":{"id":"$PROD_ID","object":"product","name":"Team","price":20000,"currency":"USD","billing_type":"recurring","billing_period":"every-month","status":"active","tax_mode":"exclusive","tax_category":"saas","mode":"test"},"customer":{"id":"$CUST_ID","object":"customer","email":"$EMAIL","name":"Test User","country":"PK","mode":"test"},"collection_method":"charge_automatically","status":"past_due","current_period_end_date":"2026-09-01T00:00:00.000Z","canceled_at":null,"metadata":{"safely_user_id":"$USER_ID"},"mode":"test"},"id":"evt_manual_test_$(date +%s)","created_at":$(date +%s)}
JSON
)

SIGNATURE=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')

echo ""
echo "Sending real, signed past_due event to your local server..."
curl -X POST http://localhost:3000/api/v1/webhooks/creem \
  -H "Content-Type: application/json" \
  -H "creem-signature: $SIGNATURE" \
  -d "$PAYLOAD"
echo ""
