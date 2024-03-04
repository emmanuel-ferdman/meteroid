use chrono::{Datelike, Months, NaiveDate, NaiveDateTime};
use std::error::Error;

use crate::helpers;
use testcontainers::clients::Cli;
use testcontainers::Container;
use testcontainers_modules::postgres::Postgres;
use time::macros::datetime;
use tonic::Code;

use meteroid::db::get_connection;
use meteroid::mapping::common::{chrono_to_date, chrono_to_datetime};
use meteroid::models::InvoiceLine;
use meteroid_grpc::meteroid::api;

use crate::meteroid_it;
use crate::meteroid_it::clients::AllClients;
use crate::meteroid_it::container::{MeteroidSetup, SeedLevel};
use meteroid_grpc::meteroid::api::shared::v1::BillingPeriod;
use meteroid_grpc::meteroid::api::subscriptions::v1::cancel_subscription_request::EffectiveAt;
use meteroid_grpc::meteroid::api::subscriptions::v1::SubscriptionStatus;
use meteroid_grpc::meteroid::api::users::v1::UserRole;

struct TestContext<'a> {
    setup: MeteroidSetup,
    clients: AllClients,
    _container: Container<'a, Postgres>,
}

async fn setup_test<'a>(
    docker: &'a Cli,
    seed_level: SeedLevel,
) -> Result<TestContext<'a>, Box<dyn Error>> {
    helpers::init::logging();
    let (_container, postgres_connection_string) = meteroid_it::container::start_postgres(&docker);
    let setup =
        meteroid_it::container::start_meteroid(postgres_connection_string, seed_level).await;

    let auth = meteroid_it::svc_auth::login(setup.channel.clone()).await;
    assert_eq!(auth.user.unwrap().role, UserRole::Admin as i32);

    let clients = AllClients::from_channel(
        setup.channel.clone(),
        auth.token.clone().as_str(),
        "a712afi5lzhk",
    );

    Ok(TestContext {
        setup,
        clients,
        _container,
    })
}

#[tokio::test]
async fn test_subscription_create() {
    let docker = Cli::default();
    let TestContext {
        setup,
        clients,
        _container,
    } = setup_test(&docker, SeedLevel::PLANS).await.unwrap();
    let conn = get_connection(&setup.pool).await.unwrap();

    let tenant_id = "018c2c82-3df1-7e84-9e05-6e141d0e751a".to_string();
    let customer_id = "018c345f-7324-7cd2-a692-78e5ab9158e0".to_string();
    let plan_version_id = "018c344b-da87-7392-bbae-c5c8780adb1b".to_string();
    let component_id = "018c344c-9ec9-7608-b115-1537b6985e73".to_string();

    let now = chrono::offset::Local::now().date_naive();

    let subscription = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(now.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![
                        api::subscriptions::v1::subscription_parameters::SubscriptionParameter {
                            component_id: component_id.clone(),
                            value: 10,
                        },
                    ],
                    committed_billing_period: Some(0),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    // it should fail if a parameter is missing
    let res = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(now.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![],
                    committed_billing_period: Some(0),
                }),
            },
        ))
        .await;

    let err = res.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);

    let res = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(now.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![
                        api::subscriptions::v1::subscription_parameters::SubscriptionParameter {
                            component_id: component_id.clone(),
                            value: 10,
                        },
                    ],
                    committed_billing_period: None,
                }),
            },
        ))
        .await;

    let err = res.err().unwrap();
    assert_eq!(err.code(), Code::InvalidArgument);

    let result_subscription = clients
        .subscriptions
        .clone()
        .get_subscription_details(tonic::Request::new(
            api::subscriptions::v1::GetSubscriptionDetailsRequest {
                subscription_id: subscription.subscription.clone().unwrap().id.clone(),
            },
        ))
        .await
        .unwrap()
        .into_inner()
        .subscription
        .unwrap();

    // check DB state
    assert_eq!(
        result_subscription.customer_id.clone().to_string(),
        customer_id.clone()
    );
    assert_eq!(
        result_subscription.plan_version_id.to_string(),
        plan_version_id
    );

    let db_invoices = meteroid_repository::invoices::get_invoices_to_issue()
        .bind(&conn, &1)
        .all()
        .await
        .unwrap();

    assert_eq!(db_invoices.len(), 1);

    let db_invoice = db_invoices.get(0).unwrap();

    assert_eq!(db_invoice.tenant_id.to_string(), tenant_id);
    assert_eq!(db_invoice.customer_id.clone().to_string(), customer_id);
    assert_eq!(
        db_invoice.subscription_id.to_string(),
        subscription.subscription.clone().unwrap().id
    );

    // teardown
    meteroid_it::container::terminate_meteroid(setup.token, setup.join_handle).await
}

#[tokio::test]
async fn test_subscription_cancel() {
    let docker = Cli::default();
    let TestContext {
        setup,
        clients,
        _container,
    } = setup_test(&docker, SeedLevel::PLANS).await.unwrap();
    let customer_id = "018c345f-7324-7cd2-a692-78e5ab9158e0".to_string();
    let plan_version_id = "018c344b-da87-7392-bbae-c5c8780adb1b".to_string();
    let component_id = "018c344c-9ec9-7608-b115-1537b6985e73".to_string();

    let now = chrono::offset::Local::now().date_naive();

    let subscription = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(now.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![
                        api::subscriptions::v1::subscription_parameters::SubscriptionParameter {
                            component_id: component_id.clone(),
                            value: 10,
                        },
                    ],
                    committed_billing_period: Some(0),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    let result_subscription = clients
        .subscriptions
        .clone()
        .cancel_subscription(tonic::Request::new(
            api::subscriptions::v1::CancelSubscriptionRequest {
                subscription_id: subscription.subscription.clone().unwrap().id.clone(),
                reason: Some("test".to_string()),
                effective_at: EffectiveAt::BillingPeriodEnd as i32,
            },
        ))
        .await
        .unwrap()
        .into_inner()
        .subscription
        .unwrap();

    // check DB state
    assert_eq!(result_subscription.status(), SubscriptionStatus::Pending);
    assert!(result_subscription.canceled_at.is_some());

    // teardown
    meteroid_it::container::terminate_meteroid(setup.token, setup.join_handle).await
}

#[tokio::test]
async fn test_slot_subscription_upgrade_downgrade() {
    let docker = Cli::default();
    let TestContext {
        setup,
        clients,
        _container,
    } = setup_test(&docker, SeedLevel::PLANS).await.unwrap();

    let customer_id = "018c345f-7324-7cd2-a692-78e5ab9158e0".to_string();
    let plan_version_id = "018c344b-da87-7392-bbae-c5c8780adb1b".to_string();
    let component_id = "018c344c-9ec9-7608-b115-1537b6985e73".to_string();

    fn now() -> NaiveDateTime {
        chrono::offset::Local::now().naive_utc()
    }

    let start = now().date();

    let seats_quantity = 15;

    let subscription = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: start.day(),
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![
                        api::subscriptions::v1::subscription_parameters::SubscriptionParameter {
                            component_id: component_id.clone(),
                            value: seats_quantity,
                        },
                    ],
                    committed_billing_period: Some(BillingPeriod::Monthly.into()),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    let subscription_id =
        uuid::Uuid::parse_str(subscription.subscription.map(|s| s.id).unwrap().as_str()).unwrap();
    let price_component_id = uuid::Uuid::parse_str(component_id.as_str()).unwrap();

    let db_invoices = meteroid_it::db::invoice::all(&setup.pool).await;

    let sub_invoice_id = db_invoices.get(0).unwrap().id;

    let current_active_seats = meteroid_it::db::slot_transaction::get_active_slots(
        &setup.pool,
        subscription_id.clone(),
        price_component_id.clone(),
        chrono_to_datetime(now()).unwrap(),
    )
    .await;

    assert_eq!(current_active_seats, seats_quantity as i32);

    // downgrade -6
    let slots = clients
        .subscriptions
        .clone()
        .apply_slots_delta(tonic::Request::new(
            api::subscriptions::v1::ApplySlotsDeltaRequest {
                subscription_id: subscription_id.to_string(),
                price_component_id: price_component_id.to_string(),
                delta: -6,
            },
        ))
        .await
        .unwrap()
        .into_inner()
        .active_slots;

    assert_eq!(slots as i32, seats_quantity as i32);

    let current_active_seats = meteroid_it::db::slot_transaction::get_active_slots(
        &setup.pool,
        subscription_id.clone(),
        price_component_id.clone(),
        chrono_to_datetime(now()).unwrap(),
    )
    .await;

    assert_eq!(current_active_seats, seats_quantity as i32);

    // downgrade -10 should fail
    let slots = clients
        .subscriptions
        .clone()
        .apply_slots_delta(tonic::Request::new(
            api::subscriptions::v1::ApplySlotsDeltaRequest {
                subscription_id: subscription_id.to_string(),
                price_component_id: price_component_id.to_string(),
                delta: -10,
            },
        ))
        .await;

    assert!(slots.is_err());

    // upgrade 5
    let slots = clients
        .subscriptions
        .clone()
        .apply_slots_delta(tonic::Request::new(
            api::subscriptions::v1::ApplySlotsDeltaRequest {
                subscription_id: subscription_id.to_string(),
                price_component_id: price_component_id.to_string(),
                delta: 5,
            },
        ))
        .await
        .unwrap()
        .into_inner()
        .active_slots;

    assert_eq!(slots as i32, seats_quantity as i32 + 5);

    let current_active_seats = meteroid_it::db::slot_transaction::get_active_slots(
        &setup.pool,
        subscription_id.clone(),
        price_component_id.clone(),
        chrono_to_datetime(now()).unwrap(),
    )
    .await;

    assert_eq!(current_active_seats, seats_quantity as i32 + 5);

    let db_invoices = meteroid_it::db::invoice::all(&setup.pool)
        .await
        .into_iter()
        .filter(|i| i.id != sub_invoice_id)
        .collect::<Vec<_>>();

    assert_eq!(db_invoices.len(), 1);

    let db_invoice = db_invoices.get(0).unwrap();

    assert_eq!(db_invoice.invoice_date, chrono_to_date(start).unwrap());

    let invoice_lines: Vec<InvoiceLine> =
        serde_json::from_value(db_invoice.line_items.clone()).unwrap();
    assert_eq!(invoice_lines.len(), 1);

    let invoice_line = invoice_lines.get(0).unwrap();
    assert_eq!(invoice_line.name, "Seats");
    assert_eq!(invoice_line.quantity, Some(5));

    assert_eq!(invoice_line.unit_price, Some(1000f64));
    assert_eq!(invoice_line.total, 1000 * 5);

    let period = invoice_line.period.as_ref().unwrap();
    assert_eq!(period.from, start);
    assert_eq!(period.to, start.checked_add_months(Months::new(1)).unwrap());
}

#[tokio::test]
async fn test_subscription_create_invoice_seats() {
    let docker = Cli::default();
    let TestContext {
        setup,
        clients,
        _container,
    } = setup_test(&docker, SeedLevel::PLANS).await.unwrap();
    let customer_id = "018c345f-7324-7cd2-a692-78e5ab9158e0".to_string();
    let plan_version_id = "018c344b-da87-7392-bbae-c5c8780adb1b".to_string();
    let component_id = "018c344c-9ec9-7608-b115-1537b6985e73".to_string();

    let start = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();

    let seats_quantity = 15;

    let subscription = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 10,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![
                        api::subscriptions::v1::subscription_parameters::SubscriptionParameter {
                            component_id: component_id.clone(),
                            value: seats_quantity,
                        },
                    ],
                    committed_billing_period: Some(BillingPeriod::Monthly.into()),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    let subscription_id =
        uuid::Uuid::parse_str(subscription.subscription.map(|s| s.id).unwrap().as_str()).unwrap();
    let price_component_id = uuid::Uuid::parse_str(component_id.as_str()).unwrap();

    let db_invoices = meteroid_it::db::invoice::all(&setup.pool).await;

    assert_eq!(db_invoices.len(), 1);

    let db_invoice = db_invoices.get(0).unwrap();

    assert_eq!(db_invoice.invoice_date, chrono_to_date(start).unwrap());

    let invoice_lines: Vec<InvoiceLine> =
        serde_json::from_value(db_invoice.line_items.clone()).unwrap();
    assert_eq!(invoice_lines.len(), 1);

    let invoice_line = invoice_lines.get(0).unwrap();
    assert_eq!(invoice_line.name, "Seats");
    assert_eq!(invoice_line.quantity, Some(seats_quantity));

    // Monthly unit price (1000) * num_days (10 - 1) / total_days_in_month (31)
    let prorated_unit_price: i64 = (1000.0 * (10 - 1) as f64 / 31.0).round() as i64;
    assert_eq!(invoice_line.unit_price, Some(prorated_unit_price as f64));
    assert_eq!(
        invoice_line.total,
        prorated_unit_price * seats_quantity as i64
    );

    let period = invoice_line.period.as_ref().unwrap();
    assert_eq!(period.from, start);
    assert_eq!(period.to, start.with_day(10).unwrap());

    let current_active_seats = meteroid_it::db::slot_transaction::get_active_slots(
        &setup.pool,
        subscription_id.clone(),
        price_component_id.clone(),
        datetime!(2023-01-01 2:00),
    )
    .await;

    assert_eq!(current_active_seats, seats_quantity as i32);

    // teardown
    meteroid_it::container::terminate_meteroid(setup.token, setup.join_handle).await
}

#[tokio::test]
async fn test_subscription_create_invoice_rate() {
    let docker = Cli::default();
    let TestContext {
        setup,
        clients,
        _container,
    } = setup_test(&docker, SeedLevel::PLANS).await.unwrap();
    let conn = get_connection(&setup.pool).await.unwrap();

    let customer_id = "018c345f-7324-7cd2-a692-78e5ab9158e0".to_string();
    let plan_version_id = "018c344a-78a9-7e2b-af90-5748672711f8".to_string();

    let start = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();

    // should fail with invalid billing period
    let res = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![],
                    committed_billing_period: Some(BillingPeriod::Quarterly.into()),
                }),
            },
        ))
        .await;

    assert!(res.is_err());

    // not prorated
    let sub_annual = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![],
                    committed_billing_period: Some(BillingPeriod::Annual.into()),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    // not prorated
    let sub_monthly = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 1,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![],
                    committed_billing_period: Some(BillingPeriod::Monthly.into()),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    let sub_monthly_prorated = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 30,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![],
                    committed_billing_period: Some(BillingPeriod::Monthly.into()),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    let db_invoices = meteroid_repository::invoices::get_invoices_to_issue()
        .bind(&conn, &1)
        .all()
        .await
        .unwrap();

    assert_eq!(db_invoices.len(), 3);

    let db_invoice_monthly = db_invoices
        .iter()
        .find(|i| i.subscription_id.to_string() == sub_monthly.subscription.clone().unwrap().id)
        .unwrap();

    let invoice_lines_monthly: Vec<InvoiceLine> =
        serde_json::from_value(db_invoice_monthly.line_items.clone()).unwrap();
    assert_eq!(invoice_lines_monthly.len(), 1);
    let invoice_line_monthly = invoice_lines_monthly.get(0).unwrap();
    assert_eq!(invoice_line_monthly.name, "Subscription Rate");
    assert_eq!(invoice_line_monthly.quantity, Some(1));
    assert_eq!(invoice_line_monthly.unit_price, Some(3500.0));
    assert_eq!(invoice_line_monthly.total, 3500);

    let period = invoice_line_monthly.period.as_ref().unwrap();
    assert_eq!(period.from, start);
    assert_eq!(period.to, start.checked_add_months(Months::new(1)).unwrap());

    let db_invoice_annual = db_invoices
        .iter()
        .find(|i| i.subscription_id.to_string() == sub_annual.subscription.clone().unwrap().id)
        .unwrap();

    let invoice_lines_annual: Vec<InvoiceLine> =
        serde_json::from_value(db_invoice_annual.line_items.clone()).unwrap();
    assert_eq!(invoice_lines_annual.len(), 1);
    let invoice_line_annual = invoice_lines_annual.get(0).unwrap();
    assert_eq!(invoice_line_annual.name, "Subscription Rate");
    assert_eq!(invoice_line_annual.quantity, Some(1));
    assert_eq!(invoice_line_annual.unit_price, Some(15900.0));
    assert_eq!(invoice_line_annual.total, 15900);

    let period = invoice_line_annual.period.as_ref().unwrap();
    assert_eq!(period.from, start);
    assert_eq!(
        period.to,
        start.checked_add_months(Months::new(12)).unwrap()
    );

    // prorated
    let db_invoice_monthly = db_invoices
        .iter()
        .find(|i| {
            i.subscription_id.to_string() == sub_monthly_prorated.subscription.clone().unwrap().id
        })
        .unwrap();

    let invoice_lines_monthly: Vec<InvoiceLine> =
        serde_json::from_value(db_invoice_monthly.line_items.clone()).unwrap();
    assert_eq!(invoice_lines_monthly.len(), 1);
    let invoice_line_monthly = invoice_lines_monthly.get(0).unwrap();
    assert_eq!(invoice_line_monthly.name, "Subscription Rate");
    assert_eq!(invoice_line_monthly.quantity, Some(1));

    let prorated_unit_price: i64 = (3500.0 * (30 - 1) as f64 / 31.0).round() as i64;

    assert_eq!(
        invoice_line_monthly.unit_price,
        Some(prorated_unit_price as f64)
    );
    assert_eq!(invoice_line_monthly.total, prorated_unit_price);

    let period = invoice_line_monthly.period.as_ref().unwrap();
    assert_eq!(period.from, start);
    assert_eq!(period.to, start.with_day(30).unwrap());

    // teardown
    meteroid_it::container::terminate_meteroid(setup.token, setup.join_handle).await
}

#[tokio::test]
async fn test_subscription_create_invoice_usage() {
    let docker = Cli::default();
    let TestContext {
        setup,
        clients,
        _container,
    } = setup_test(&docker, SeedLevel::PLANS).await.unwrap();
    let conn = get_connection(&setup.pool).await.unwrap();

    let customer_id = "018c345f-7324-7cd2-a692-78e5ab9158e0".to_string();
    let plan_version_id = "018c35cc-3f41-7551-b7b6-f8bbcd62b784".to_string();
    let slots_component_id = "3b083801-c77c-4488-848e-a185f0f0a8be".to_string();

    let start = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();

    let slots_quantity = 3;

    let _subscription = clients
        .subscriptions
        .clone()
        .create_subscription(tonic::Request::new(
            api::subscriptions::v1::CreateSubscriptionRequest {
                customer_id: customer_id.clone(),
                plan_version_id: plan_version_id.clone(),
                billing_start: Some(start.into()),
                billing_end: None,
                net_terms: 0,
                billing_day: 10,
                parameters: Some(api::subscriptions::v1::SubscriptionParameters {
                    parameters: vec![
                        api::subscriptions::v1::subscription_parameters::SubscriptionParameter {
                            component_id: slots_component_id.clone(),
                            value: slots_quantity,
                        },
                    ],
                    committed_billing_period: Some(BillingPeriod::Monthly.into()),
                }),
            },
        ))
        .await
        .unwrap()
        .into_inner();

    let db_invoices = meteroid_repository::invoices::get_invoices_to_issue()
        .bind(&conn, &1)
        .all()
        .await
        .unwrap();

    assert_eq!(db_invoices.len(), 1);

    let db_invoice = db_invoices.get(0).unwrap();

    assert_eq!(db_invoice.invoice_date, chrono_to_date(start).unwrap());

    let invoice_lines: Vec<InvoiceLine> =
        serde_json::from_value(db_invoice.line_items.clone()).unwrap();

    assert_eq!(
        invoice_lines.len(),
        1,
        "Usage lines are not created in initial invoice."
    );

    let invoice_line = invoice_lines
        .iter()
        .find(|l| l.name == "Organization Slots")
        .unwrap();
    assert_eq!(invoice_line.name, "Organization Slots");
    assert_eq!(invoice_line.quantity, Some(slots_quantity));

    // Monthly unit price (1000) * num_days (10 - 1) / total_days_in_month (31)
    let prorated_unit_price: i64 = (2500.0 * (10 - 1) as f64 / 31.0).round() as i64;
    assert_eq!(invoice_line.unit_price, Some(prorated_unit_price as f64));
    assert_eq!(
        invoice_line.total,
        prorated_unit_price * slots_quantity as i64
    );

    let period = invoice_line.period.as_ref().unwrap();
    assert_eq!(period.from, start);
    assert_eq!(period.to, start.with_day(10).unwrap());

    // teardown
    meteroid_it::container::terminate_meteroid(setup.token, setup.join_handle).await
}

// TDOO capacity, onetime, recurring
