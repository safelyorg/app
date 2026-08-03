#!/bin/bash
# Sends a real, correctly-signed subscription.canceled webhook directly
# to your own local server - using YOUR real email, testing the
# genuine "payment retries exhausted" path specifically (not a
# voluntary cancellation), same reasoning as test_past_due.sh.
#
# IMPORTANT: the subscription you use here must currently show
# 'active' or 'past_due' in your database - NOT already 'canceled' -
# since your own code deliberately skips sending this email if the
# row was already canceled (that's the exact self-cancel vs.
# payment-failure distinction we built).
#
# Usage: ./test_subscription_ended.sh

read -p "Your real subscription ID (sub_...): " SUB_ID
read -p "Your real customer ID (cust_...): " CUST_ID
read -p "Your real product ID (prod_...): " PROD_ID
read -p "Your real email address: " EMAIL
read -p "Your real Safely user ID (uuid): " USER_ID
read -sp "Your real CREEM_WEBHOOK_SECRET: " SECRET
echo ""

PAYLOAD=$(cat <<JSON
{"eventType":"subscription.canceled","object":{"id":"$SUB_ID","object":"subscription","product":{"id":"$PROD_ID","object":"product","name":"Team","price":20000,"currency":"USD","billing_type":"recurring","billing_period":"every-month","status":"active","tax_mode":"exclusive","tax_category":"saas","mode":"test"},"customer":{"id":"$CUST_ID","object":"customer","email":"$EMAIL","name":"Test User","country":"PK","mode":"test"},"collection_method":"charge_automatically","status":"canceled","current_period_end_date":"2026-09-01T00:00:00.000Z","canceled_at":"2026-08-01T00:00:00.000Z","metadata":{"safely_user_id":"$USER_ID"},"mode":"test"},"id":"evt_manual_test_$(date +%s)","created_at":$(date +%s)}
JSON
)

SIGNATURE=$(echo -n "$PAYLOAD" | openssl dgst -sha256 -hmac "$SECRET" | sed 's/^.* //')

echo ""
echo "Sending real, signed subscription.canceled event to your local server..."
curl -X POST http://localhost:3000/api/v1/webhooks/creem \
  -H "Content-Type: application/json" \
  -H "creem-signature: $SIGNATURE" \
  -d "$PAYLOAD"
echo ""
