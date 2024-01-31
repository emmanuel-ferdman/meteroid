pub mod subscriptions {
    use crate::api::services::shared;
    use meteroid_grpc::meteroid::api::subscriptions::v1 as proto;
    use meteroid_repository::subscriptions as db;

    use tonic::Status;

    pub fn db_to_proto(s: db::Subscription) -> Result<proto::Subscription, Status> {
        let parameters_decoded: proto::SubscriptionParameters =
            serde_json::from_value(s.input_parameters)
                .map_err(|e| Status::internal(format!("Failed to decode parameters: {}", e)))?;

        Ok(proto::Subscription {
            id: s.subscription_id.to_string(),
            tenant_id: s.tenant_id.to_string(),
            customer_id: s.customer_id.to_string(),
            plan_version_id: s.plan_version_id.to_string(),
            parameters: Some(parameters_decoded),
            net_terms: s.net_terms,
            currency: s.currency,
            version: s.version as u32,
            billing_end_date: s.billing_end_date.map(shared::mapping::date::to_proto),
            billing_start_date: Some(shared::mapping::date::to_proto(s.billing_start_date)),
            customer_name: s.customer_name,
        })
    }

    pub fn list_db_to_proto(s: db::SubscriptionList) -> Result<proto::Subscription, Status> {
        let parameters_decoded: proto::SubscriptionParameters =
            serde_json::from_value(s.input_parameters)
                .map_err(|e| Status::internal(format!("Failed to decode parameters: {}", e)))?;

        Ok(proto::Subscription {
            id: s.subscription_id.to_string(),
            tenant_id: s.tenant_id.to_string(),
            customer_id: s.customer_id.to_string(),
            plan_version_id: s.plan_version_id.to_string(),
            parameters: Some(parameters_decoded),
            net_terms: s.net_terms,
            currency: s.currency,
            version: s.version as u32,
            billing_end_date: s.billing_end_date.map(shared::mapping::date::to_proto),
            billing_start_date: Some(shared::mapping::date::to_proto(s.billing_start_date)),
            customer_name: s.customer_name,
        })
    }
}