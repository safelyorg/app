-- One scam report per person per seller.
--
-- The server already refuses a second report from the same person
-- (handlers/fraud_reports.rs). This database rule also stops two
-- reports that arrive at the same moment.
--
-- Reports from deleted accounts (user_id is NULL) are not limited.
--
-- Nothing is deleted. If some person already has two reports for the
-- same seller, the rule is skipped and a notice is printed - remove the
-- repeats by hand, then add the rule again.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM fraud_reports
        WHERE user_id IS NOT NULL AND seller_id IS NOT NULL
        GROUP BY user_id, seller_id
        HAVING COUNT(*) > 1
    ) THEN
        RAISE NOTICE 'Repeat reports exist: the one-report-per-person rule was not added.';
    ELSE
        CREATE UNIQUE INDEX IF NOT EXISTS uq_fraud_reports_user_seller
            ON fraud_reports (user_id, seller_id)
            WHERE user_id IS NOT NULL AND seller_id IS NOT NULL;
    END IF;
END
$$;
