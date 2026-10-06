#!/bin/bash
# Sends a real, correctly-signed subscription.paid webhook to your
# LOCAL server - the same event Creem sends when someone pays for a
# plan or a plan renews. Use it to test monthly and yearly plans
# without paying through checkout.
#
# Sending it again with a later period end = a renewal (scans go back
# to 0). Sending a yearly product while a monthly plan is active = a
# switch to yearly (the monthly plan is ended).
#
# Usage:
# sed -i 's/\r$//' scripts/test_subscription_paid.sh
# chmod +x scripts/test_subscription_paid.sh
# ./scripts/test_subscription_paid.sh

read -p "Subscription ID (any new value, e.g. sub_test_1): " SUB_ID
read -p "Customer ID (any value, e.g. cust_test_1): " CUST_ID
read -p "Product ID (one of the 4 CREEM_*_PRODUCT_ID values in .env): " PROD_ID
read -p "Plan name (Team or Enterprise): " PLAN_NAME
read -p "Billing (month or year): " BILLING
read -p "Price in cents (Team 20000/144000, Enterprise 50000/360000): " PRICE
read -p "Your email address: " EMAIL
read -p "Your Safely user ID (uuid, from the users table): " USER_ID
read -sp "Your CREEM_WEBHOOK_SECRET: " SECRET
echo ""

if [ "$BILLING" = "year" ]; then
  PERIOD="every-year"
  PERIOD_END=$(date -u -d "+1 year" +%Y-%m-%dT%H:%M:%S.000Z)
else
  PERIOD="every-month"
  PERIOD_END=$(date -u -d "+1 month" +%Y-%m-%dT%H:%M:%S.000Z)
fi
PERIOD_START=$(date -u +%Y-%m-%dT%H:%M:%S.000Z)
NOW=$(date +%s)

PAYLOAD=$(cat <<JSON
{"eventType":"subscription.paid","object":{"id":"$SUB_ID","object":"subscription","product":{"id":"$PROD_ID","object":"product","name":"$PLAN_NAME","price":$PRICE,"currency":"USD","billing_type":"recurring","billing_period":"$PERIOD","status":"active","tax_mode":"exclusive","tax_category":"saas","mode":"test"},"customer":{"id":"$CUST_ID","object":"customer","email":"$EMAIL","name":"Test User","country":"PK","mode":"test"},"collection_method":"charge_automatically","status":"active","last_transaction_id":"tran_manual_test_$NOW","current_period_start_date":"$PERIOD_START","current_period_end_date":"$PERIOD_END","canceled_at":null,"metadata":{"safely_user_id":"$USER_ID"},"mode":"test"},"id":"evt_manual_test_$NOW","created_at":$NOW}
JSON
)

SIGNATURE=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')

echo ""
echo "Sending a signed subscription.paid event ($PLAN_NAME, $BILLING) to your local server..."
curl -X POST http://localhost:3000/api/v1/webhooks/creem \
  -H "Content-Type: application/json" \
  -H "creem-signature: $SIGNATURE" \
  -d "$PAYLOAD"
echo ""
