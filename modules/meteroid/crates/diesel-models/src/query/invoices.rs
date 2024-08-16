use crate::errors::IntoDbResult;
use crate::invoices::{
    DetailedInvoiceRow, InvoiceRow, InvoiceRowLinesPatch, InvoiceRowNew, InvoiceWithCustomerRow,
};
use chrono::NaiveDateTime;

use crate::{DbResult, PgConn};

use crate::enums::{InvoiceExternalStatusEnum, InvoiceStatusEnum, InvoicingProviderEnum};
use crate::extend::cursor_pagination::{
    CursorPaginate, CursorPaginatedVec, CursorPaginationRequest,
};
use crate::extend::order::OrderByRequest;
use crate::extend::pagination::{Paginate, PaginatedVec, PaginationRequest};
use diesel::dsl::IntervalDsl;
use diesel::{
    debug_query, BoolExpressionMethods, JoinOnDsl, NullableExpressionMethods,
    PgTextExpressionMethods, SelectableHelper,
};
use diesel::{ExpressionMethods, QueryDsl};
use error_stack::ResultExt;

impl InvoiceRowNew {
    pub async fn insert(&self, conn: &mut PgConn) -> DbResult<InvoiceRow> {
        use crate::schema::invoice::dsl::*;
        use diesel_async::RunQueryDsl;

        let query = diesel::insert_into(invoice).values(self);

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .get_result(conn)
            .await
            .attach_printable("Error while inserting invoice")
            .into_db_result()
    }
}

impl InvoiceRow {
    pub async fn find_by_id(
        conn: &mut PgConn,
        param_tenant_id: uuid::Uuid,
        param_invoice_id: uuid::Uuid,
    ) -> DbResult<DetailedInvoiceRow> {
        use crate::schema::customer::dsl as c_dsl;
        use crate::schema::invoice::dsl as i_dsl;
        use crate::schema::plan::dsl as p_dsl;
        use crate::schema::plan_version::dsl as pv_dsl;
        use crate::schema::product_family::dsl as pf_dsl;
        use crate::schema::subscription::dsl as s_dsl;

        use diesel_async::RunQueryDsl;

        let query = i_dsl::invoice
            .inner_join(c_dsl::customer.on(i_dsl::customer_id.eq(c_dsl::id)))
            .left_join(s_dsl::subscription.on(i_dsl::subscription_id.eq(s_dsl::id.nullable())))
            .left_join(pv_dsl::plan_version.on(s_dsl::plan_version_id.eq(pv_dsl::id)))
            .left_join(p_dsl::plan.on(pv_dsl::plan_id.eq(p_dsl::id)))
            .left_join(pf_dsl::product_family.on(p_dsl::product_family_id.eq(pf_dsl::id)))
            .filter(i_dsl::tenant_id.eq(param_tenant_id))
            .filter(i_dsl::id.eq(param_invoice_id))
            .select(DetailedInvoiceRow::as_select());

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .first(conn)
            .await
            .attach_printable("Error while finding invoice by id")
            .into_db_result()
    }

    pub async fn list(
        conn: &mut PgConn,
        param_tenant_id: uuid::Uuid,
        param_customer_id: Option<uuid::Uuid>,
        param_status: Option<InvoiceStatusEnum>,
        param_query: Option<String>,
        order_by: OrderByRequest,
        pagination: PaginationRequest,
    ) -> DbResult<PaginatedVec<InvoiceWithCustomerRow>> {
        use crate::schema::customer::dsl as c_dsl;
        use crate::schema::invoice::dsl as i_dsl;

        let mut query = i_dsl::invoice
            .inner_join(c_dsl::customer.on(i_dsl::customer_id.eq(c_dsl::id)))
            .filter(i_dsl::tenant_id.eq(param_tenant_id))
            .select(InvoiceWithCustomerRow::as_select())
            .into_boxed();

        if let Some(param_customer_id) = param_customer_id {
            query = query.filter(i_dsl::customer_id.eq(param_customer_id))
        }

        if let Some(param_status) = param_status {
            query = query.filter(i_dsl::status.eq(param_status))
        }

        if let Some(param_query) = param_query {
            query = query.filter(c_dsl::name.ilike(format!("%{}%", param_query)))
        }

        match order_by {
            OrderByRequest::IdAsc => query = query.order(i_dsl::id.asc()),
            OrderByRequest::IdDesc => query = query.order(i_dsl::id.desc()),
            OrderByRequest::DateAsc => query = query.order(i_dsl::created_at.asc()),
            OrderByRequest::DateDesc => query = query.order(i_dsl::created_at.desc()),
            _ => query = query.order(i_dsl::id.asc()),
        }

        let paginated_query = query.paginate(pagination);

        log::debug!(
            "{}",
            debug_query::<diesel::pg::Pg, _>(&paginated_query).to_string()
        );

        paginated_query
            .load_and_count_pages(conn)
            .await
            .attach_printable("Error while fetching invoices")
            .into_db_result()
    }

    pub async fn insert_invoice_batch(
        conn: &mut PgConn,
        invoices: Vec<InvoiceRowNew>,
    ) -> DbResult<Vec<InvoiceRow>> {
        use crate::schema::invoice::dsl::*;
        use diesel_async::RunQueryDsl;

        let query = diesel::insert_into(invoice).values(&invoices);

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .get_results(conn)
            .await
            .attach_printable("Error while inserting invoice")
            .into_db_result()
    }

    pub async fn update_external_status(
        conn: &mut PgConn,
        id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        external_status: InvoiceExternalStatusEnum,
    ) -> DbResult<usize> {
        use crate::schema::invoice::dsl as i_dsl;
        use diesel_async::RunQueryDsl;

        let query = diesel::update(i_dsl::invoice)
            .filter(i_dsl::id.eq(id))
            .filter(i_dsl::tenant_id.eq(tenant_id))
            .set((
                i_dsl::external_status.eq(external_status),
                i_dsl::updated_at.eq(chrono::Utc::now().naive_utc()),
            ));

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .attach_printable("Error while update invoice external_status")
            .into_db_result()
    }

    pub async fn list_to_finalize(
        conn: &mut PgConn,
        pagination: CursorPaginationRequest,
    ) -> DbResult<CursorPaginatedVec<InvoiceRow>> {
        use crate::schema::invoice::dsl as i_dsl;
        use crate::schema::invoicing_config::dsl as ic_dsl;

        let query = i_dsl::invoice
            .inner_join(ic_dsl::invoicing_config.on(i_dsl::tenant_id.eq(ic_dsl::tenant_id)))
            .filter(
                i_dsl::status.ne_all(vec![InvoiceStatusEnum::Void, InvoiceStatusEnum::Finalized]),
            )
            .filter(diesel::dsl::now.gt(i_dsl::invoice_date
                + diesel::dsl::sql::<diesel::sql_types::Interval>(
                    "\"invoicing_config\".\"grace_period_hours\" * INTERVAL '1 hour'",
                )))
            .select(InvoiceRow::as_select())
            .cursor_paginate(pagination, i_dsl::id, "id");

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .load_and_get_next_cursor(conn, |a| a.id)
            .await
            .attach_printable("Error while paginating invoices to finalize")
            .into_db_result()
    }

    pub async fn finalize(
        conn: &mut PgConn,
        id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> DbResult<usize> {
        use crate::schema::invoice::dsl as i_dsl;
        use diesel_async::RunQueryDsl;

        let now = chrono::Utc::now().naive_utc();

        let query = diesel::update(i_dsl::invoice)
            .filter(i_dsl::id.eq(id))
            .filter(i_dsl::tenant_id.eq(tenant_id))
            .filter(
                i_dsl::status.ne_all(vec![InvoiceStatusEnum::Finalized, InvoiceStatusEnum::Void]),
            )
            .set((
                i_dsl::status.eq(InvoiceStatusEnum::Finalized),
                i_dsl::updated_at.eq(now),
                i_dsl::data_updated_at.eq(now),
                i_dsl::finalized_at.eq(now),
            ));

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .attach_printable("Error while finalizing invoice")
            .into_db_result()
    }

    pub async fn list_outdated(
        conn: &mut PgConn,
        pagination: CursorPaginationRequest,
    ) -> DbResult<CursorPaginatedVec<InvoiceRow>> {
        use crate::schema::invoice::dsl as i_dsl;

        let query = i_dsl::invoice
            .filter(
                i_dsl::status.ne_all(vec![InvoiceStatusEnum::Void, InvoiceStatusEnum::Finalized]),
            )
            .filter(
                i_dsl::data_updated_at
                    .is_null()
                    .or(diesel::dsl::now.gt(i_dsl::invoice_date + 1.hour())),
            )
            .select(InvoiceRow::as_select())
            .cursor_paginate(pagination, i_dsl::id, "id");

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .load_and_get_next_cursor(conn, |a| a.id)
            .await
            .attach_printable("Error while paginating outdated invoices")
            .into_db_result()
    }

    pub async fn list_to_issue(
        conn: &mut PgConn,
        max_attempts: i32,
        pagination: CursorPaginationRequest,
    ) -> DbResult<CursorPaginatedVec<InvoiceRow>> {
        use crate::schema::invoice::dsl as i_dsl;

        let query = i_dsl::invoice
            .filter(i_dsl::status.eq(InvoiceStatusEnum::Finalized))
            .filter(i_dsl::invoicing_provider.ne(InvoicingProviderEnum::Manual))
            .filter(i_dsl::issued.eq(false))
            .filter(i_dsl::issue_attempts.lt(max_attempts))
            .select(InvoiceRow::as_select())
            .cursor_paginate(pagination, i_dsl::id, "id");

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .load_and_get_next_cursor(conn, |a| a.id)
            .await
            .attach_printable("Error while paginating invoices to issue")
            .into_db_result()
    }

    pub async fn issue_success(
        conn: &mut PgConn,
        id: uuid::Uuid,
        tenant_id: uuid::Uuid,
    ) -> DbResult<usize> {
        use crate::schema::invoice::dsl as i_dsl;
        use diesel_async::RunQueryDsl;

        let now = chrono::Utc::now().naive_utc();

        let query = diesel::update(i_dsl::invoice)
            .filter(i_dsl::id.eq(id))
            .filter(i_dsl::tenant_id.eq(tenant_id))
            .filter(i_dsl::status.eq(InvoiceStatusEnum::Finalized))
            .filter(i_dsl::issued.eq(false))
            .set((
                i_dsl::issued.eq(true),
                i_dsl::issue_attempts.eq(i_dsl::issue_attempts + 1),
                i_dsl::updated_at.eq(now),
                i_dsl::last_issue_attempt_at.eq(now),
            ));

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .attach_printable("Error while issue_success invoice")
            .into_db_result()
    }

    pub async fn issue_error(
        conn: &mut PgConn,
        id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        last_issue_error: &str,
    ) -> DbResult<usize> {
        use crate::schema::invoice::dsl as i_dsl;
        use diesel_async::RunQueryDsl;

        let now = chrono::Utc::now().naive_utc();

        let query = diesel::update(i_dsl::invoice)
            .filter(i_dsl::id.eq(id))
            .filter(i_dsl::tenant_id.eq(tenant_id))
            .filter(i_dsl::status.eq(InvoiceStatusEnum::Finalized))
            .filter(i_dsl::issued.eq(false))
            .set((
                i_dsl::last_issue_error.eq(last_issue_error),
                i_dsl::issue_attempts.eq(i_dsl::issue_attempts + 1),
                i_dsl::updated_at.eq(now),
                i_dsl::last_issue_attempt_at.eq(now),
            ));

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .attach_printable("Error while issue_error invoice")
            .into_db_result()
    }

    pub async fn update_pending_finalization(
        conn: &mut PgConn,
        now: NaiveDateTime,
    ) -> DbResult<usize> {
        use diesel_async::RunQueryDsl;

        // diesel doesn't support update/delete with joins https://github.com/diesel-rs/diesel/issues/1478
        // also the id::eq_any(subquery_with_joins) doesn't work when the subquery is on the same table
        let raw_sql = r#"
UPDATE invoice
SET status = 'PENDING',
    updated_at = $1
FROM invoicing_config
WHERE invoice.tenant_id = invoicing_config.tenant_id
  AND invoice.status = 'DRAFT'
  AND invoice.invoice_date < $2
  AND $3 <= (invoice.invoice_date + interval '1 hour' * invoicing_config.grace_period_hours);
        "#;

        let query = diesel::sql_query(raw_sql)
            .bind::<diesel::sql_types::Timestamp, _>(now)
            .bind::<diesel::sql_types::Timestamp, _>(now)
            .bind::<diesel::sql_types::Timestamp, _>(now);

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .attach_printable("Error while fetching revenue trend")
            .into_db_result()
    }
}

impl InvoiceRowLinesPatch {
    pub async fn update_lines(
        &self,
        id: uuid::Uuid,
        tenant_id: uuid::Uuid,
        conn: &mut PgConn,
    ) -> DbResult<usize> {
        use crate::schema::invoice::dsl as i_dsl;
        use diesel_async::RunQueryDsl;

        let query = diesel::update(i_dsl::invoice)
            .filter(i_dsl::id.eq(id).and(i_dsl::tenant_id.eq(tenant_id)))
            .set(self);

        log::debug!("{}", debug_query::<diesel::pg::Pg, _>(&query).to_string());

        query
            .execute(conn)
            .await
            .attach_printable("Error while updating invoice lines")
            .into_db_result()
    }
}
