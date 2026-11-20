// Vendor checks (services/features/vendors.rs): paid plans only, the
// link saved only as a hash, the vendor's form (draft, email code,
// send), one scan per vendor, "please fix" without a new scan, the
// buyer's decision, Watch, and waiting when the plan has no scans left.
// Uses the real test database. The vendor uses a free email and no
// website, so no website is looked up.

mod common;

use crate::common::{
    TestPaidPlan, cleanup_test_subscription, cleanup_test_user, create_test_user,
    insert_subscription_with_scans, test_pool,
};
use backend::models::fraud_reports::ReportTypes;
use backend::services::features::history::get_user_reports;
use backend::services::features::team::invite;
use backend::services::features::team_history::HistoryWho;
use backend::services::features::vendors::{
    AskRequest, DecisionRequest, EMAILS_PER_ADDRESS_PER_DAY, InviteRequest, NewVendor,
    ReportRequest, VendorError, ask_vendor, cancel_vendor, confirm_email_code, decide,
    download_document, hash_code, hash_token, invite_vendors, may_email_vendor, new_vendor_link,
    open_form, report_vendor, run_check, run_vendor_jobs, save_draft, submit_form, upload_document,
    vendor_detail, vendor_list, watch_vendor,
};
use serde_json::{Value, json};
use serial_test::serial;
use sqlx::{Pool, Postgres, query, query_scalar};
use uuid::Uuid;

const OWNER: &str = "v-owner@vendor-test.safely.sh";
const FREE: &str = "v-free@vendor-test.safely.sh";
const MEMBER: &str = "v-member@vendor-test.safely.sh";
const SUB: &str = "sub_vendor_test";

async fn cleanup(pool: &Pool<Postgres>) {
    let _ = query("DELETE FROM watched_suppliers WHERE team_owner_id IN (SELECT id FROM users WHERE email LIKE '%@vendor-test.safely.sh')")
        .execute(pool)
        .await;
    let _ = query("DELETE FROM fraud_reports WHERE platform = 'vendor' AND user_id IN (SELECT id FROM users WHERE email LIKE '%@vendor-test.safely.sh')")
        .execute(pool)
        .await;
    let _ = query("DELETE FROM vendor_checks WHERE team_owner_id IN (SELECT id FROM users WHERE email LIKE '%@vendor-test.safely.sh')")
        .execute(pool)
        .await;
    let _ = query("DELETE FROM sellers WHERE platform = 'vendor' AND name LIKE 'Vendortest%'")
        .execute(pool)
        .await;
    cleanup_test_subscription(pool, SUB).await;
    let _ = query("DELETE FROM team_members WHERE email LIKE '%@vendor-test.safely.sh'")
        .execute(pool)
        .await;
    for email in [OWNER, FREE, MEMBER] {
        cleanup_test_user(pool, email).await;
    }
}

fn one_vendor() -> InviteRequest {
    InviteRequest {
        buyer_company: "Buyer Test Co".into(),
        language: Some("pt-br".into()),
        send_email: Some(false),
        vendors: vec![NewVendor {
            name: "Vendortest Comércio Ltda".into(),
            email: "vendortest.sales@gmail.com".into(),
            country: Some("Brazil".into()),
            phone: Some("+55 11 91234-5678".into()),
            website: None,
        }],
    }
}

fn token_of(link: &str) -> String {
    link.rsplit('/').next().unwrap().to_string()
}

async fn scans_used(pool: &Pool<Postgres>) -> i32 {
    query_scalar(
        "SELECT scans_used_this_period FROM subscriptions WHERE creem_subscription_id = $1",
    )
    .bind(SUB)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Confirms the vendor's email the way the form does (the code email
/// itself needs Resend, so the code is written straight in).
async fn confirm_email(pool: &Pool<Postgres>, id: Uuid, token: &str, email: &str) {
    query(
        "UPDATE vendor_checks SET email_code_hash = $2, email_code_for = $3,
                email_code_sent_at = NOW(), email_code_tries = 0 WHERE id = $1",
    )
    .bind(id)
    .bind(hash_code(id, "123456"))
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
    assert!(matches!(
        confirm_email_code(pool, token, "000000").await,
        Err(VendorError::CodeWrong)
    ));
    assert_eq!(
        confirm_email_code(pool, token, "123 456").await.unwrap(),
        email
    );
}

#[tokio::test]
#[serial]
async fn a_vendor_fills_the_form_and_the_buyer_gets_the_result() {
    let pool = test_pool().await;
    cleanup(&pool).await;
    let (owner, _) = create_test_user(&pool, OWNER).await;
    let (free, _) = create_test_user(&pool, FREE).await;

    // Free plan: no vendor checks.
    assert!(matches!(
        invite_vendors(&pool, free.id, one_vendor()).await,
        Err(VendorError::NeedsPaidPlan)
    ));
    assert_eq!(
        vendor_list(&pool, free.id, HistoryWho::Mine).await.unwrap()["paid"],
        false
    );

    insert_subscription_with_scans(
        &pool,
        owner.id,
        SUB,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 0,
            bought_days_ago: 3,
        },
    )
    .await;

    let invited = invite_vendors(&pool, owner.id, one_vendor()).await.unwrap();
    let first = &invited["invited"][0];
    let id: Uuid = serde_json::from_value(first["id"].clone()).unwrap();
    let link = first["link"].as_str().unwrap().to_string();
    assert!(link.contains("/v/"));
    assert_eq!(first["email_sent"], false);
    let token = token_of(&link);

    // Only the hash is saved.
    let saved: String =
        query_scalar("SELECT token_hash FROM vendor_check_links WHERE vendor_check_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, hash_token(&token));
    assert_ne!(saved, token);

    // The vendor opens the form: no result, the buyer's name, the language.
    let form = open_form(&pool, &token).await.unwrap();
    assert_eq!(form["buyer_company"], "Buyer Test Co");
    assert_eq!(form["language"], "pt");
    assert_eq!(form["done"], false);
    assert!(form.get("result").is_none());
    assert!(form.get("risk_level").is_none());
    assert_eq!(
        vendor_detail(&pool, owner.id, id).await.unwrap()["status"],
        "opened"
    );

    // Progress is saved; unknown fields are left out.
    save_draft(
        &pool,
        &token,
        &json!({"company_name": "Vendortest Comercio", "password": "x"}),
    )
    .await
    .unwrap();
    let form = open_form(&pool, &token).await.unwrap();
    assert_eq!(form["answers"]["company_name"], "Vendortest Comercio");
    assert!(form["answers"].get("password").is_none());

    let answers = json!({
        "company_name": "Vendortest Comércio Ltda",
        "country": "Brazil",
        "company_number": "12.345.678/0001-95",
        "email": "vendortest.sales@gmail.com",
        "phone": "(11) 91234-5678",
        "bank_holder": "Carlos Pereira",
    });
    // Not sent before the email is confirmed.
    assert!(matches!(
        submit_form(&pool, &token, &answers).await,
        Err(VendorError::EmailNotConfirmed)
    ));
    // Not sent with an answer missing.
    let mut missing = answers.clone();
    missing["bank_holder"] = json!("");
    assert!(matches!(
        submit_form(&pool, &token, &missing).await,
        Err(VendorError::BadInput("answers_missing"))
    ));

    confirm_email(&pool, id, &token, "vendortest.sales@gmail.com").await;
    let sent = submit_form(&pool, &token, &answers).await.unwrap();
    assert_eq!(sent, id);
    assert_eq!(open_form(&pool, &token).await.unwrap()["done"], true);

    run_check(&pool, id).await;
    assert_eq!(scans_used(&pool).await, 1, "one vendor = one scan");
    let detail = vendor_detail(&pool, owner.id, id).await.unwrap();
    assert_eq!(detail["status"], "ready");
    let cards = detail["result"]["cards"].as_array().unwrap();
    let kind = |key: &str| {
        cards
            .iter()
            .find(|c| c["key"] == key)
            .map(|c| c["type"].as_str().unwrap().to_string())
    };
    assert_eq!(kind("email_code").as_deref(), Some("good"));
    assert_eq!(kind("free_email").as_deref(), Some("caution"));
    assert_eq!(kind("bank_name").as_deref(), Some("bad"));
    assert_eq!(kind("known_all").as_deref(), Some("good"));
    assert!(detail["risk_score"].as_i64().unwrap() >= 25);

    // Ask for new bank details: only that answer opens, no new scan.
    let fix_link = ask_vendor(
        &pool,
        owner.id,
        id,
        AskRequest {
            reason: Some("bank_change".into()),
            note: Some("Use the company account".into()),
            ..AskRequest::default()
        },
    )
    .await
    .unwrap();
    // The old link stopped.
    assert!(matches!(
        open_form(&pool, &token).await,
        Err(VendorError::LinkClosed)
    ));
    let fix = token_of(&fix_link);
    let form = open_form(&pool, &fix).await.unwrap();
    assert_eq!(
        form["editable"],
        json!(["bank_holder", "bank_account", "bank_code"])
    );
    assert_eq!(form["ask_note"], "Use the company account");
    // Other answers can't be changed.
    submit_form(
        &pool,
        &fix,
        &json!({"bank_holder": "Vendortest Comercio LTDA", "company_name": "Someone Else"}),
    )
    .await
    .unwrap();
    run_check(&pool, id).await;
    assert_eq!(scans_used(&pool).await, 1, "a vendor's changes are free");
    let detail = vendor_detail(&pool, owner.id, id).await.unwrap();
    assert_eq!(detail["vendor_name"], "Vendortest Comércio Ltda");
    assert_eq!(detail["previous"].as_array().unwrap().len(), 1);
    let cards = detail["result"]["cards"].as_array().unwrap();
    let bank = cards.iter().find(|c| c["key"] == "bank_name").unwrap();
    assert_eq!(bank["type"], "good");

    decide(
        &pool,
        owner.id,
        id,
        DecisionRequest {
            decision: "approve".into(),
            note: Some("Called them".into()),
        },
    )
    .await
    .unwrap();
    let detail = vendor_detail(&pool, owner.id, id).await.unwrap();
    assert_eq!(detail["status"], "approved");
    assert_eq!(detail["decided_by"], OWNER);

    watch_vendor(&pool, owner.id, id).await.unwrap();
    watch_vendor(&pool, owner.id, id).await.unwrap();
    let watched: i64 =
        query_scalar("SELECT COUNT(*) FROM watched_suppliers WHERE team_owner_id = $1")
            .bind(owner.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(watched, 1);

    // Someone else's team can't open it.
    assert!(matches!(
        vendor_detail(&pool, free.id, id).await,
        Err(VendorError::NotFound)
    ));

    let list = vendor_list(&pool, owner.id, HistoryWho::Mine)
        .await
        .unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["counts"]["approved"], 1);
    assert_eq!(list["items"][0]["watching"], true);
    assert_eq!(list["paid"], true);
    assert_eq!(
        vendor_detail(&pool, owner.id, id).await.unwrap()["full_result"],
        true
    );

    // The plan ends: the vendors are still listed and open in short
    // (score, level, top 3 reasons); the rest and every action need a
    // paid plan again.
    query("UPDATE subscriptions SET status = 'canceled' WHERE user_id = $1")
        .bind(owner.id)
        .execute(&pool)
        .await
        .unwrap();
    let list = vendor_list(&pool, owner.id, HistoryWho::Mine)
        .await
        .unwrap();
    assert_eq!(list["paid"], false);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let short = vendor_detail(&pool, owner.id, id).await.unwrap();
    assert_eq!(short["full_result"], false);
    assert!(short["risk_score"].is_number());
    assert_eq!(short["status"], "approved");
    assert_eq!(short["answers"], json!([]));
    assert_eq!(short["previous"], json!([]));
    assert_eq!(short["result"]["records"], json!([]));
    assert!(short["result"].get("social_candidates").is_none());
    let cards = short["result"]["cards"].as_array().unwrap();
    assert!(!cards.is_empty() && cards.len() <= 3);
    assert!(cards.iter().all(|c| c["type"] != "info"));
    assert!(short["result"]["counts"]["total"].as_u64().unwrap() > cards.len() as u64);
    assert!(matches!(
        decide(
            &pool,
            owner.id,
            id,
            DecisionRequest {
                decision: "reject".into(),
                note: None,
            },
        )
        .await,
        Err(VendorError::NeedsPaidPlan)
    ));
    assert!(matches!(
        invite_vendors(&pool, owner.id, one_vendor()).await,
        Err(VendorError::NeedsPaidPlan)
    ));

    cleanup(&pool).await;
}

#[tokio::test]
#[serial]
async fn links_close_and_checks_wait_for_scans() {
    let pool = test_pool().await;
    cleanup(&pool).await;
    let (owner, _) = create_test_user(&pool, OWNER).await;
    insert_subscription_with_scans(
        &pool,
        owner.id,
        SUB,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 500,
            bought_days_ago: 3,
        },
    )
    .await;

    // Cancel: the link stops.
    let invited = invite_vendors(&pool, owner.id, one_vendor()).await.unwrap();
    let id: Uuid = serde_json::from_value(invited["invited"][0]["id"].clone()).unwrap();
    let token = token_of(invited["invited"][0]["link"].as_str().unwrap());
    cancel_vendor(&pool, owner.id, id).await.unwrap();
    assert!(matches!(
        open_form(&pool, &token).await,
        Err(VendorError::LinkClosed)
    ));
    // A new link opens it again.
    let again = token_of(&new_vendor_link(&pool, owner.id, id).await.unwrap());
    assert_eq!(open_form(&pool, &again).await.unwrap()["done"], false);

    // Too old: expired by the background job.
    query("UPDATE vendor_checks SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        open_form(&pool, &again).await,
        Err(VendorError::LinkClosed)
    ));
    run_vendor_jobs(&pool).await;
    assert_eq!(
        vendor_detail(&pool, owner.id, id).await.unwrap()["status"],
        "expired"
    );

    // No scans left: the check waits, nothing is counted.
    let fresh = token_of(&new_vendor_link(&pool, owner.id, id).await.unwrap());
    confirm_email(&pool, id, &fresh, "vendortest.sales@gmail.com").await;
    let answers: Value = json!({
        "company_name": "Vendortest Comércio Ltda",
        "country": "Brazil",
        "company_number": "12.345.678/0001-95",
        "phone": "(11) 91234-5678",
        "bank_holder": "Vendortest Comercio Ltda",
    });
    submit_form(&pool, &fresh, &answers).await.unwrap();
    run_check(&pool, id).await;
    assert_eq!(
        vendor_detail(&pool, owner.id, id).await.unwrap()["status"],
        "no_scans"
    );
    assert_eq!(scans_used(&pool).await, 500);

    // Scans back: the waiting check runs.
    query("UPDATE subscriptions SET scans_used_this_period = 0 WHERE creem_subscription_id = $1")
        .bind(SUB)
        .execute(&pool)
        .await
        .unwrap();
    run_check(&pool, id).await;
    assert_eq!(
        vendor_detail(&pool, owner.id, id).await.unwrap()["status"],
        "ready"
    );
    assert_eq!(scans_used(&pool).await, 1);

    cleanup(&pool).await;
}

#[tokio::test]
#[serial]
async fn the_form_page_and_a_closed_link() {
    use axum::{body::Body, http::Request, http::StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let pool = test_pool().await;
    let app = backend::routes::vendors::vendors_routes().with_state(pool);
    let page = app
        .clone()
        .oneshot(Request::get("/v/abc123").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    let html = page.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&html);
    assert!(html.contains("Quick supplier check"));
    assert!(html.contains("noindex"));

    let closed = app
        .clone()
        .oneshot(
            Request::get("/api/v1/vendor-form/not-a-real-link")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(closed.status(), StatusCode::GONE);

    let signed_out = app
        .oneshot(Request::get("/api/v1/vendors").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(signed_out.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[serial]
async fn report_files_and_a_shared_bank_account() {
    let pool = test_pool().await;
    cleanup(&pool).await;
    let (owner, _) = create_test_user(&pool, OWNER).await;
    insert_subscription_with_scans(
        &pool,
        owner.id,
        SUB,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 0,
            bought_days_ago: 3,
        },
    )
    .await;

    // Two vendors, different companies, the same bank account.
    let mut ids = Vec::new();
    for (company, holder) in [
        ("Vendortest Alpha Ltda", "Vendortest Alpha Ltda"),
        ("Vendortest Beta Ltda", "Vendortest Beta Ltda"),
    ] {
        // A different email for each (the same email waits for one check).
        let email = format!(
            "{}@vendortest-mail.com",
            company.split(' ').nth(1).unwrap().to_lowercase()
        );
        let mut req = one_vendor();
        req.vendors[0].email = email.clone();
        let invited = invite_vendors(&pool, owner.id, req).await.unwrap();
        let id: Uuid = serde_json::from_value(invited["invited"][0]["id"].clone()).unwrap();
        let token = token_of(invited["invited"][0]["link"].as_str().unwrap());
        if ids.is_empty() {
            // Files: only real PDF / JPG / PNG / WebP, at most 5 MB.
            assert!(matches!(
                upload_document(&pool, &token, "registration", "x.exe", b"MZ not a pdf").await,
                Err(VendorError::BadFile)
            ));
            assert!(matches!(
                upload_document(&pool, &token, "passport", "x.pdf", b"%PDF-1.7 x").await,
                Err(VendorError::BadInput(_))
            ));
            upload_document(&pool, &token, "registration", "cnpj.pdf", b"%PDF-1.7 test")
                .await
                .unwrap();
            assert_eq!(
                open_form(&pool, &token).await.unwrap()["documents"][0]["file_name"],
                "cnpj.pdf"
            );
        }
        confirm_email(&pool, id, &token, &email).await;
        submit_form(
            &pool,
            &token,
            &json!({
                "company_name": company,
                "country": "Brazil",
                "company_number": "12.345.678/0001-95",
                "phone": "+55 11 91234-5678",
                "bank_holder": holder,
                "bank_account": "DE89 3704 0044 0532 0130 00",
                "bank_code": "COBADEFFXXX",
            }),
        )
        .await
        .unwrap();
        run_check(&pool, id).await;
        ids.push(id);
    }

    let detail = vendor_detail(&pool, owner.id, ids[1]).await.unwrap();
    let cards = detail["result"]["cards"].as_array().unwrap();
    let find = |key: &str| cards.iter().find(|c| c["key"] == key).cloned().unwrap();
    assert_eq!(find("same_account")["type"], "bad");
    assert!(
        find("same_account")["detail"]
            .as_str()
            .unwrap()
            .contains("Vendortest Alpha")
    );
    assert_eq!(find("bank_account")["value"], "Account abroad");
    assert_eq!(find("phone_country")["type"], "good");
    // Different emails: no "email used for another company".
    assert_eq!(find("same_email")["type"], "good");

    // The first vendor's file: listed for the buyer, and downloadable.
    let detail = vendor_detail(&pool, owner.id, ids[0]).await.unwrap();
    let doc = &detail["documents"][0];
    assert_eq!(doc["kind"], "registration");
    let doc_id: Uuid = serde_json::from_value(doc["id"].clone()).unwrap();
    let (name, content_type, data) = download_document(&pool, owner.id, ids[0], doc_id)
        .await
        .unwrap();
    assert_eq!(
        (name.as_str(), content_type.as_str()),
        ("cnpj.pdf", "application/pdf")
    );
    assert_eq!(data, b"%PDF-1.7 test");
    // Not through another vendor's id.
    assert!(
        download_document(&pool, owner.id, ids[1], doc_id)
            .await
            .is_err()
    );

    // Report the vendor: once, and it shows in My Reports as a vendor.
    report_vendor(
        &pool,
        owner.id,
        ids[1],
        ReportRequest {
            report_type: ReportTypes::NoDelivery,
            description: Some("Never shipped".into()),
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        report_vendor(
            &pool,
            owner.id,
            ids[1],
            ReportRequest {
                report_type: ReportTypes::Scam,
                description: None
            }
        )
        .await,
        Err(VendorError::AlreadyReported)
    ));
    // My Reports: a vendor's own website is offered next to its page.
    query(r#"UPDATE vendor_checks SET answers = answers || '{"website": "vendortest-beta.com"}' WHERE id = $1"#)
        .bind(ids[1])
        .execute(&pool)
        .await
        .unwrap();
    let reports = get_user_reports(&pool, owner.id).await.unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].platform, "vendor");
    assert_eq!(
        reports[0].vendor_website.as_deref(),
        Some("vendortest-beta.com")
    );
    assert_eq!(reports[0].scan_id, None);
    assert_eq!(
        reports[0].listing_url.as_deref(),
        Some(format!("/vendors/{}", ids[1]).as_str())
    );
    let detail = vendor_detail(&pool, owner.id, ids[1]).await.unwrap();
    assert_eq!(detail["reported_by_you"], true);
    // The side card: all Safely reports on the company, and who invited.
    assert_eq!(detail["report_count"], 1);
    assert_eq!(detail["invited_by"], OWNER);
    assert_eq!(detail["invited_by_you"], true);
    // Reporting is not watching.
    assert_eq!(detail["watching"], false);

    // The PDF report is made from the same data as the page.
    let pdf = backend::services::features::vendor_pdf::generate_vendor_pdf(&detail).unwrap();
    assert!(pdf.starts_with(b"%PDF"));

    cleanup(&pool).await;
}

#[test]
fn my_reports_marks_vendors_and_marketplace_scans() {
    let mut context = tera::Context::new();
    context.insert(
        "items",
        &json!([
            {"reported_at_formatted": "2026-11-19", "report_type": "no_delivery", "report_label": "Took payment and never shipped",
             "platform": "vendor", "seller_name": "Acme", "listing_url": "/vendors?open=1",
             "history_url": null, "website": "https://acme.com/"},
            {"reported_at_formatted": "2026-11-18", "report_type": "scam", "report_label": "Scam",
             "platform": "alibaba", "seller_name": "Boyue", "listing_url": "https://www.alibaba.com/x",
             "history_url": "/dashboard/alibaba/1234", "website": null},
            {"reported_at_formatted": "2026-11-17", "report_type": "scam", "report_label": "Scam",
             "platform": "vendor", "seller_name": "NoSite", "listing_url": "/vendors/2",
             "history_url": null, "website": null}
        ]),
    );
    let html = backend::services::features::templates::get_tera()
        .render("report_rows.html", &context)
        .unwrap();
    assert!(html.contains("data-report-kind=\"vendor\""));
    assert!(html.contains("data-report-kind=\"marketplace\""));
    // A vendor opens its own page in the dashboard, not a new tab, and
    // its website (when given) in a new tab.
    assert!(html.contains("href=\"/vendors?open=1\" data-report-open=\"vendor\""));
    assert!(html.contains("href=\"https://acme.com/\" target=\"_blank\""));
    assert!(html.contains("Vendor form"));
    // A marketplace seller opens its saved scan in History, and the
    // listing on Alibaba in a new tab.
    assert!(html.contains("href=\"/dashboard/alibaba/1234\" data-report-open=\"history\""));
    assert!(html.contains("href=\"https://www.alibaba.com/x\" target=\"_blank\""));
    // The reason in words, with its key for the translation.
    assert!(html.contains("data-report-reason=\"no_delivery\">Took payment and never shipped"));
    // 5 columns: date, reason, platform, seller, original page.
    assert_eq!(html.matches("<td").count(), 3 * 5);
    // A vendor with no website: only its page.
    assert_eq!(html.matches("data-report-link=\"website\"").count(), 1);
}

#[tokio::test]
#[serial]
async fn one_open_check_per_email_and_a_daily_email_limit() {
    let pool = test_pool().await;
    cleanup(&pool).await;
    let _ = query("DELETE FROM vendor_email_sends WHERE email LIKE '%@vendortest-limit.com'")
        .execute(&pool)
        .await;
    let (owner, _) = create_test_user(&pool, OWNER).await;
    insert_subscription_with_scans(
        &pool,
        owner.id,
        SUB,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 0,
            bought_days_ago: 3,
        },
    )
    .await;

    // The same email twice in one list, then again: one check.
    let mut req = one_vendor();
    req.vendors.push(req.vendors[0].clone());
    let first = invite_vendors(&pool, owner.id, req).await.unwrap();
    assert_eq!(first["invited"].as_array().unwrap().len(), 1);
    let again = invite_vendors(&pool, owner.id, one_vendor()).await.unwrap();
    assert_eq!(again["invited"][0]["already"], true);
    assert_eq!(again["invited"][0]["id"], first["invited"][0]["id"]);
    assert!(again["invited"][0]["link"].is_null());

    // After a decision (here: canceled) the vendor can be invited again.
    let id: Uuid = serde_json::from_value(first["invited"][0]["id"].clone()).unwrap();
    cancel_vendor(&pool, owner.id, id).await.unwrap();
    let later = invite_vendors(&pool, owner.id, one_vendor()).await.unwrap();
    assert!(later["invited"][0].get("already").is_none());

    // At most 3 Safely emails a day to one address.
    for _ in 0..EMAILS_PER_ADDRESS_PER_DAY {
        assert!(may_email_vendor(&pool, "Ana@VendorTest-Limit.com").await);
    }
    assert!(!may_email_vendor(&pool, "ana@vendortest-limit.com").await);
    assert!(may_email_vendor(&pool, "bob@vendortest-limit.com").await);
    let _ = query("DELETE FROM vendor_email_sends WHERE email LIKE '%@vendortest-limit.com'")
        .execute(&pool)
        .await;
    cleanup(&pool).await;
}

#[tokio::test]
#[serial]
async fn the_vendor_list_follows_my_vendors_team_and_one_person() {
    let pool = test_pool().await;
    cleanup(&pool).await;
    let (owner, _) = create_test_user(&pool, OWNER).await;
    insert_subscription_with_scans(
        &pool,
        owner.id,
        SUB,
        TestPaidPlan {
            plan_name: "Team",
            status: "active",
            billing_interval: "month",
            scans_used: 0,
            bought_days_ago: 3,
        },
    )
    .await;
    invite(&pool, owner.id, MEMBER).await.unwrap();
    let (member, _) = create_test_user(&pool, MEMBER).await;

    // The member invites one vendor; the owner another.
    invite_vendors(&pool, member.id, one_vendor())
        .await
        .unwrap();
    let mut req = one_vendor();
    req.vendors[0].email = "owner.vendor@vendortest-mail.com".into();
    invite_vendors(&pool, owner.id, req).await.unwrap();
    let count = |v: &Value| v["items"].as_array().unwrap().len();

    // Owner: my vendors, all team vendors, the member's vendors.
    let mine = vendor_list(&pool, owner.id, HistoryWho::Mine)
        .await
        .unwrap();
    assert_eq!(count(&mine), 1);
    assert_eq!(mine["team"], true);
    assert_eq!(mine["can_see_all"], true);
    assert_eq!(mine["people"][0]["email"], MEMBER);
    assert_eq!(
        count(
            &vendor_list(&pool, owner.id, HistoryWho::AllTeam)
                .await
                .unwrap()
        ),
        2
    );
    let theirs = vendor_list(&pool, owner.id, HistoryWho::Person(member.id))
        .await
        .unwrap();
    assert_eq!(count(&theirs), 1);
    assert_eq!(theirs["who"], member.id.to_string());

    // Member: no "all team"; falls back to my vendors. Can see the owner's.
    let all = vendor_list(&pool, member.id, HistoryWho::AllTeam)
        .await
        .unwrap();
    assert_eq!(all["who"], "me");
    assert_eq!(count(&all), 1);
    assert_eq!(all["can_see_all"], false);
    assert_eq!(
        count(
            &vendor_list(&pool, member.id, HistoryWho::Person(owner.id))
                .await
                .unwrap()
        ),
        1
    );
    // Someone outside the team: my vendors.
    let stranger = vendor_list(&pool, owner.id, HistoryWho::Person(Uuid::new_v4()))
        .await
        .unwrap();
    assert_eq!(stranger["who"], "me");

    cleanup(&pool).await;
}
