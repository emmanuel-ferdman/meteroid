pub mod plans {
    use crate::api::domain_mapping::billing_period::to_proto;
    use meteroid_grpc::meteroid::api::plans::v1::{
        plan_billing_configuration as billing_config_grpc, ListPlanVersion, PlanOverview,
    };
    use meteroid_grpc::meteroid::api::plans::v1::{
        ListPlan, ListSubscribablePlanVersion, Plan, PlanBillingConfiguration, PlanDetails,
        PlanStatus, PlanType, PlanVersion, TrialConfig,
    };
    use meteroid_store::domain;
    use meteroid_store::domain::enums::{PlanStatusEnum, PlanTypeEnum};

    pub struct PlanDetailsWrapper(pub PlanDetails);
    pub struct PlanVersionWrapper(pub PlanVersion);
    pub struct PlanTypeWrapper(pub PlanType);
    pub struct PlanStatusWrapper(pub PlanStatus);
    pub struct ListPlanWrapper(pub ListPlan);
    pub struct ListSubscribablePlanVersionWrapper(pub ListSubscribablePlanVersion);
    pub struct ListPlanVersionWrapper(pub ListPlanVersion);
    pub struct PlanOverviewWrapper(pub PlanOverview);

    impl From<domain::PlanVersion> for ListPlanVersionWrapper {
        fn from(value: domain::PlanVersion) -> Self {
            Self(ListPlanVersion {
                id: value.id.to_string(),
                is_draft: value.is_draft_version,
                version: value.version as u32,
                currency: value.currency,
            })
        }
    }

    impl From<domain::PlanVersion> for PlanVersionWrapper {
        fn from(value: domain::PlanVersion) -> Self {
            fn trial_config(version: &domain::PlanVersion) -> Option<TrialConfig> {
                Some(TrialConfig {
                    duration_in_days: version.trial_duration_days? as u32,
                    fallback_plan_id: version.trial_fallback_plan_id?.to_string(),
                })
            }

            fn billing_config(version: &domain::PlanVersion) -> Option<PlanBillingConfiguration> {
                Some(PlanBillingConfiguration {
                    billing_periods: version
                        .billing_periods
                        .clone()
                        .into_iter()
                        .map(|freq| to_proto(freq) as i32)
                        .collect(),
                    billing_cycles: Some(match version.billing_cycles {
                        Some(count) => {
                            billing_config_grpc::BillingCycles::Fixed(billing_config_grpc::Fixed {
                                count: count as u32,
                            })
                        }
                        None => billing_config_grpc::BillingCycles::Forever(
                            billing_config_grpc::Forever {},
                        ),
                    }),
                    net_terms: version.net_terms as u32,
                    service_period_start: Some(match version.period_start_day {
                        Some(day) => billing_config_grpc::ServicePeriodStart::DayOfMonth(
                            billing_config_grpc::DayOfMonth {
                                day_of_month: day as u32,
                            },
                        ),
                        None => billing_config_grpc::ServicePeriodStart::SubscriptionAnniversary(
                            billing_config_grpc::SubscriptionAnniversary {},
                        ),
                    }),
                })
            }
            Self(PlanVersion {
                id: value.id.to_string(),
                version: value.version as u32,
                is_draft: value.is_draft_version,
                trial_config: trial_config(&value),
                billing_config: billing_config(&value),
                currency: value.currency,
            })
        }
    }

    impl From<domain::FullPlan> for PlanDetailsWrapper {
        fn from(value: domain::FullPlan) -> Self {
            Self(PlanDetails {
                plan: Some(Plan {
                    id: value.plan.id.to_string(),
                    external_id: value.plan.external_id,
                    name: value.plan.name,
                    description: value.plan.description,
                    plan_type: PlanTypeWrapper::from(value.plan.plan_type).0 as i32,
                    plan_status: PlanStatusWrapper::from(value.plan.status).0 as i32,
                }),
                current_version: Some(PlanVersionWrapper::from(value.version).0),
                metadata: vec![],
            })
        }
    }

    impl Into<PlanTypeEnum> for PlanTypeWrapper {
        fn into(self) -> PlanTypeEnum {
            match self.0 {
                PlanType::Standard => PlanTypeEnum::Standard,
                PlanType::Free => PlanTypeEnum::Free,
                PlanType::Custom => PlanTypeEnum::Custom,
            }
        }
    }

    impl From<PlanTypeEnum> for PlanTypeWrapper {
        fn from(e: PlanTypeEnum) -> Self {
            Self(match e {
                PlanTypeEnum::Standard => PlanType::Standard,
                PlanTypeEnum::Free => PlanType::Free,
                PlanTypeEnum::Custom => PlanType::Custom,
            })
        }
    }

    impl Into<PlanStatusEnum> for PlanStatusWrapper {
        fn into(self) -> PlanStatusEnum {
            match self.0 {
                PlanStatus::Draft => PlanStatusEnum::Draft,
                PlanStatus::Active => PlanStatusEnum::Active,
                PlanStatus::Archived => PlanStatusEnum::Archived,
                PlanStatus::Inactive => PlanStatusEnum::Inactive,
            }
        }
    }

    impl From<PlanStatusEnum> for PlanStatusWrapper {
        fn from(e: PlanStatusEnum) -> Self {
            Self(match e {
                PlanStatusEnum::Draft => PlanStatus::Draft,
                PlanStatusEnum::Active => PlanStatus::Active,
                PlanStatusEnum::Archived => PlanStatus::Archived,
                PlanStatusEnum::Inactive => PlanStatus::Inactive,
            })
        }
    }

    impl From<domain::PlanForList> for ListPlanWrapper {
        fn from(value: domain::PlanForList) -> Self {
            Self(ListPlan {
                id: value.id.to_string(),
                name: value.name,
                external_id: value.external_id,
                description: value.description,
                plan_type: PlanTypeWrapper::from(value.plan_type).0 as i32,
                plan_status: PlanStatusWrapper::from(value.status).0 as i32,
                product_family_id: value.product_family_id.to_string(),
                product_family_name: value.product_family_name,
            })
        }
    }

    impl From<domain::PlanVersionLatest> for ListSubscribablePlanVersionWrapper {
        fn from(value: domain::PlanVersionLatest) -> Self {
            Self(ListSubscribablePlanVersion {
                id: value.id.to_string(),
                plan_id: value.plan_id.to_string(),
                plan_name: value.plan_name,
                version: value.version,
                created_by: value.created_by.to_string(),
                trial_duration_days: value.trial_duration_days,
                trial_fallback_plan_id: value.trial_fallback_plan_id.map(|x| x.to_string()),
                period_start_day: value.period_start_day.map(|x| x as i32),
                net_terms: value.net_terms,
                currency: value.currency,
                product_family_id: value.product_family_id.to_string(),
                product_family_name: value.product_family_name,
            })
        }
    }

    impl From<domain::PlanWithVersion> for PlanOverviewWrapper {
        fn from(value: domain::PlanWithVersion) -> Self {
            Self(PlanOverview {
                plan_id: value.plan.id.to_string(),
                plan_version_id: value.version.id.to_string(),
                name: value.plan.name,
                version: value.version.version as u32,
                description: value.plan.description,
                currency: value.version.currency,
                net_terms: value.version.net_terms as u32,
                billing_periods: value
                    .version
                    .billing_periods
                    .into_iter()
                    .map(|freq| to_proto(freq) as i32)
                    .collect(),
                is_draft: value.version.is_draft_version,
            })
        }
    }

    // pub mod parameters {
    //     use meteroid_grpc::meteroid::api::plans::v1 as grpc;
    //
    //     use crate::api::pricecomponents::ext::PlanParameter;
    //
    //     pub fn to_grpc(param: PlanParameter) -> grpc::PlanParameter {
    //         let param = match param {
    //             PlanParameter::BillingPeriodTerm => grpc::plan_parameter::Param::BillingPeriodTerm(
    //                 grpc::plan_parameter::BillingPeriodTerm {},
    //             ),
    //             PlanParameter::CapacityThresholdValue {
    //                 capacity_values,
    //                 component_id,
    //             } => grpc::plan_parameter::Param::CapacityThresholdValue(
    //                 grpc::plan_parameter::CapacityThresholdValue {
    //                     component_id,
    //                     capacity_values,
    //                 },
    //             ),
    //             PlanParameter::CommittedSlot { component_id } => {
    //                 grpc::plan_parameter::Param::CommittedSlot(
    //                     grpc::plan_parameter::CommittedSlot { component_id },
    //                 )
    //             }
    //         };
    //
    //         grpc::PlanParameter { param: Some(param) }
    //     }
    // }
}
