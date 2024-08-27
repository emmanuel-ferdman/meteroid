use diesel_async::scoped_futures::ScopedFutureExt;
use error_stack::Report;
use tracing_log::log;
use uuid::Uuid;

use common_eventbus::Event;
use common_utils::rng::BASE62_ALPHABET;
use diesel_models::enums::OrganizationUserRole;
use diesel_models::organization_members::OrganizationMemberRow;
use diesel_models::organizations::{OrganizationRow, OrganizationRowNew};
use diesel_models::tenants::TenantRow;

use crate::domain::enums::TenantEnvironmentEnum;
use crate::domain::{
    InstanceFlags, InvoicingEntityNew, Organization, OrganizationNew, OrganizationWithTenants,
    TenantNew,
};
use crate::errors::StoreError;
use crate::store::Store;
use crate::StoreResult;

#[async_trait::async_trait]
pub trait OrganizationsInterface {
    async fn insert_organization(
        &self,
        organization: OrganizationNew,
        actor: Uuid,
    ) -> StoreResult<OrganizationWithTenants>;

    async fn get_instance(&self) -> StoreResult<InstanceFlags>;
    async fn organization_get_or_create_invite_link(
        &self,
        organization_id: Uuid,
    ) -> StoreResult<String>;

    async fn list_organizations_for_user(&self, user_id: Uuid) -> StoreResult<Vec<Organization>>;
    async fn get_organization_by_id(&self, id: Uuid) -> StoreResult<Organization>;
    async fn get_organizations_with_tenants_by_id(
        &self,
        id: Uuid,
    ) -> StoreResult<OrganizationWithTenants>;
    async fn get_organizations_by_slug(&self, slug: String) -> StoreResult<Organization>;
}

#[async_trait::async_trait]
impl OrganizationsInterface for Store {
    async fn insert_organization(
        &self,
        organization: OrganizationNew,
        user_id: Uuid,
    ) -> StoreResult<OrganizationWithTenants> {
        let mut conn = self.get_conn().await?;

        if !self.settings.multi_organization_enabled {
            let count = OrganizationRow::count_all(&mut conn)
                .await
                .map_err(Into::<Report<StoreError>>::into)?;

            if count > 0 {
                return Err(StoreError::InvalidArgument(
                    "This instance does not allow mutiple organizations".to_string(),
                )
                .into());
            }
        }

        let org = OrganizationRowNew {
            id: Uuid::now_v7(),
            slug: Organization::new_slug(),
            trade_name: organization.trade_name.clone(),
            default_country: organization.country.clone(),
        };

        // TODO trigger sandbox init ?

        let org_member = OrganizationMemberRow {
            user_id,
            organization_id: org.id,
            role: OrganizationUserRole::Admin,
        };

        let tenant_new = TenantNew {
            name: "Production".to_string(),
            environment: TenantEnvironmentEnum::Production,
        };

        let (org_created, tenant_created) = self
            .transaction_with(&mut conn, |conn| {
                async move {
                    let org_created = OrganizationRowNew::insert(&org, conn)
                        .await
                        .map_err(Into::<Report<StoreError>>::into)?;

                    OrganizationMemberRow::insert(&org_member, conn)
                        .await
                        .map_err(Into::<Report<StoreError>>::into)?;

                    let tenant_created = self
                        .internal
                        .insert_tenant_with_default_entities(
                            conn,
                            tenant_new,
                            org.id,
                            org.trade_name.clone(),
                            org.default_country.clone(),
                            vec![],
                            organization
                                .invoicing_entity
                                .unwrap_or(InvoicingEntityNew::default()),
                        )
                        .await?;

                    Ok((org_created, tenant_created))
                }
                .scope_boxed()
            })
            .await?;

        let _ = self
            .eventbus
            .publish(Event::organization_created(user_id, org_created.id.clone()))
            .await;

        Ok(OrganizationWithTenants {
            organization: org_created.into(),
            tenants: vec![tenant_created.into()],
        })
    }

    async fn get_instance(&self) -> StoreResult<InstanceFlags> {
        let mut conn = self.get_conn().await?;

        if self.settings.multi_organization_enabled {
            Ok(InstanceFlags {
                multi_organization_enabled: true,
                instance_initiated: true,
            })
        } else {
            // single organization
            let count = OrganizationRow::count_all(&mut conn)
                .await
                .map_err(Into::<Report<StoreError>>::into)?;

            Ok(InstanceFlags {
                multi_organization_enabled: false,
                instance_initiated: count > 0,
            })
        }
    }

    async fn organization_get_or_create_invite_link(
        &self,
        organization_id: Uuid,
    ) -> StoreResult<String> {
        let mut conn = self.get_conn().await?;

        let org = OrganizationRow::get_by_id(&mut conn, organization_id)
            .await
            .map_err(Into::<Report<StoreError>>::into)?;

        match org.invite_link_hash {
            Some(hash) => Ok(hash),
            None => {
                log::warn!("Organization invite link is not set - creating new one");

                let invite_hash = nanoid::nanoid!(32, &BASE62_ALPHABET);

                let _ = OrganizationRow::update_invite_link(&mut conn, org.id, &invite_hash)
                    .await
                    .map_err(Into::<Report<StoreError>>::into)?;

                Ok(invite_hash)
            }
        }
    }

    async fn list_organizations_for_user(&self, user_id: Uuid) -> StoreResult<Vec<Organization>> {
        let mut conn = self.get_conn().await?;

        let orgs = OrganizationRow::list_by_user_id(&mut conn, user_id)
            .await
            .map_err(Into::<Report<StoreError>>::into)?;

        Ok(orgs.into_iter().map(Into::into).collect())
    }

    async fn get_organization_by_id(&self, id: Uuid) -> StoreResult<Organization> {
        let mut conn = self.get_conn().await?;

        let org = OrganizationRow::get_by_id(&mut conn, id)
            .await
            .map_err(Into::<Report<StoreError>>::into)?;

        Ok(org.into())
    }

    async fn get_organizations_with_tenants_by_id(
        &self,
        id: Uuid,
    ) -> StoreResult<OrganizationWithTenants> {
        let mut conn = self.get_conn().await?;

        let org = OrganizationRow::get_by_id(&mut conn, id)
            .await
            .map_err(Into::<Report<StoreError>>::into)?;

        let tenants = TenantRow::list_by_organization_id(&mut conn, id).await?;

        Ok(OrganizationWithTenants {
            organization: org.into(),
            tenants: tenants.into_iter().map(Into::into).collect(),
        })
    }

    async fn get_organizations_by_slug(&self, slug: String) -> StoreResult<Organization> {
        let mut conn = self.get_conn().await?;

        let org = OrganizationRow::find_by_slug(&mut conn, slug)
            .await
            .map_err(Into::<Report<StoreError>>::into)?;

        Ok(org.into())
    }
}
