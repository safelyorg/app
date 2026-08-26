use std::sync::OnceLock;
use tera::Tera;

static TERA: OnceLock<Tera> = OnceLock::new();

const EMAIL_TEMPLATE: &str = include_str!("../templates/email.html");
const WELCOME_EMAIL_TEMPLATE: &str = include_str!("../templates/welcome_email.html");
const PAYMENT_FAILED_TEMPLATE: &str = include_str!("../templates/payment_failed_email.html");
const SUBSCRIPTION_CANCELED_TEMPLATE: &str =
    include_str!("../templates/subscription_canceled_email.html");
const SUBSCRIPTION_ENDED_TEMPLATE: &str =
    include_str!("../templates/subscription_ended_email.html");
const HISTORY_ROWS_TEMPLATE: &str = include_str!("../templates/history_rows.html");

/// It builds one shared "template engine" containing every real
/// template this app uses - both real email templates, and HTML
/// fragments rendered directly for the dashboard.
///
/// TERA is a special kind of variable (OnceLock) that can only ever be
/// set up once, globally, for your entire running program. get_or_init
/// means: "if this has already been built before, just hand back that
/// same one. If it hasn't been built yet, run this code block right
/// now to build it." It creates a brand-new, empty template engine,
/// then loads each real template into it, one at a time, giving each
/// one a name - the return value is the whole, complete engine, once
/// every template is loaded.
pub fn get_tera() -> &'static Tera {
    TERA.get_or_init(|| {
        let mut tera = Tera::default();
        tera.add_raw_template("email.html", EMAIL_TEMPLATE)
            .expect("tera should add the sign-in raw template");
        tera.add_raw_template("welcome_email.html", WELCOME_EMAIL_TEMPLATE)
            .expect("tera should add the welcome raw template");
        tera.add_raw_template("payment_failed_email.html", PAYMENT_FAILED_TEMPLATE)
            .expect("tera should add the payment failed raw template");
        tera.add_raw_template("subscription_ended_email.html", SUBSCRIPTION_ENDED_TEMPLATE)
            .expect("tera should add the subscription ended raw template");
        tera.add_raw_template(
            "subscription_canceled_email.html",
            SUBSCRIPTION_CANCELED_TEMPLATE,
        )
        .expect("tera should add the subscription canceled raw template");
        tera.add_raw_template("history_rows.html", HISTORY_ROWS_TEMPLATE)
            .expect("tera should add the history rows raw template");

        tera
    })
}
