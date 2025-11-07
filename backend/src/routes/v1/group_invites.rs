use std::sync::Arc;

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Extension, Json,
};
use axum_valid::Valid;
use backend_storage::{
    group_invite::{GroupInvite, GroupInviteCreateRequest, GroupInviteStorage},
    group_join_request::GroupJoinRequestStorage,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{
    middleware::AuthenticatedUser,
    types::{AppError, Environment},
};

/// Maximum TTL for group invites (10 years)
const MAX_TTL_SECS: i64 = 10 * 366 * 24 * 60 * 60;

/// Request to create a new group invite
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateGroupInviteRequest {
    /// Topic for the group invite
    #[validate(length(min = 1))]
    pub topic: String,

    /// Name of the group
    #[validate(length(min = 1, max = 100))]
    pub group_name: String,

    /// Maximum number of uses for this invite (optional)
    #[validate(range(min = 1))]
    pub max_uses: Option<i64>,

    /// Expiration timestamp (optional)
    #[validate(custom(function = "validate_expires_at"))]
    #[schemars(description = "Unix timestamp in seconds (max 10 years from now)")]
    pub expires_at: Option<i64>,
}

/// Query parameters for getting group invites by topic
#[derive(Debug, Deserialize, JsonSchema, Validate)]
pub struct GetGroupInvitesByTopicQuery {
    /// Topic to filter invites by
    #[validate(length(min = 1))]
    pub topic: String,
}

/// Response when getting a group invite
#[derive(Debug, Serialize, JsonSchema)]
pub struct GroupInviteResponse {
    /// Unique ID of the invite
    pub id: String,
    /// Name of the group
    pub group_name: String,
    /// Invite link url
    pub link_url: String,
    /// Number of uses left
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses_left: Option<i64>,
    /// Expiration timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl GroupInviteResponse {
    /// Create a new `GroupInviteResponse` from components
    ///
    /// Conditionally calculates `uses_left` if `max_uses` is set
    ///
    /// # Errors
    /// Returns an error if there is a database error.
    pub async fn new(
        group_invite: GroupInvite,
        group_join_request_storage: Arc<GroupJoinRequestStorage>,
        invite_link_base_url: &str,
    ) -> Result<Self, AppError> {
        // TODO: Replace with actual path
        let link_url = format!(
            "{}/joinchat/{}",
            invite_link_base_url.trim_end_matches('/'),
            group_invite.id
        );

        let uses_left = if let Some(max_uses) = group_invite.max_uses {
            let used_count = group_join_request_storage
                .count_approved_by_group_invite_id(&group_invite.id)
                .await?;
            Some(max_uses.saturating_sub(used_count.into()))
        } else {
            None
        };

        Ok(Self {
            id: group_invite.id,
            group_name: group_invite.group_name,
            link_url,
            uses_left,
            expires_at: group_invite.expires_at,
        })
    }
}

// Custom validator for expiration time
fn validate_expires_at(expires_at: i64) -> Result<(), validator::ValidationError> {
    let now = chrono::Utc::now().timestamp();

    // Must be in the future
    if expires_at <= now {
        let mut error = validator::ValidationError::new("invalid_expires_at");
        error.message = Some(std::borrow::Cow::Borrowed(
            "Expiration time must be in the future",
        ));
        return Err(error);
    }

    // Must be less than max TTL
    if expires_at > now + MAX_TTL_SECS {
        let mut error = validator::ValidationError::new("invalid_expires_at");
        error.message = Some(std::borrow::Cow::Borrowed(
            "Expiration time must be less than 10 years in the future",
        ));
        return Err(error);
    }

    Ok(())
}

/// Create a new group invite
///
/// Creates a new group invite that can be shared with users to join a group.
/// The invite can optionally have a maximum number of uses and an expiration time.
///
/// # Arguments
///
/// * `user` - The authenticated user creating the invite
/// * `payload` - Request containing invite details
///
/// # Returns
///
/// Returns `201 CREATED` with the created invite details on success
///
/// # Errors
///
/// Returns an error if:
/// - `400 BAD_REQUEST` - Invalid request parameters
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
/// - `503 SERVICE_UNAVAILABLE` - Database connectivity issues
pub async fn create_group_invite(
    user: AuthenticatedUser,
    Extension(env): Extension<Environment>,
    Extension(group_invite_storage): Extension<Arc<GroupInviteStorage>>,
    Extension(group_join_request_storage): Extension<Arc<GroupJoinRequestStorage>>,
    Valid(Json(payload)): Valid<Json<CreateGroupInviteRequest>>,
) -> Result<Json<GroupInviteResponse>, AppError> {
    let create_request = GroupInviteCreateRequest {
        topic: payload.topic,
        group_name: payload.group_name,
        creator_encrypted_push_id: user.encrypted_push_id,
        max_uses: payload.max_uses,
        expires_at: payload.expires_at,
    };

    let group_invite = group_invite_storage.create(create_request).await?;

    let response = GroupInviteResponse::new(
        group_invite,
        group_join_request_storage,
        &env.invite_link_base_url(),
    )
    .await?;

    Ok(Json(response))
}

/// Get a group invite by ID
///
/// Retrieves details about a specific group invite.
///
/// # Arguments
///
/// * `id` - Path parameter containing the invite ID
///
/// # Returns
///
/// Returns `200 OK` with the invite details on success
///
/// # Errors
///
/// Returns an error if:
/// - `404 NOT_FOUND` - Invite with the given ID does not exist
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
/// - `503 SERVICE_UNAVAILABLE` - Database connectivity issues
pub async fn get_group_invite(
    Path(id): Path<String>,
    Extension(env): Extension<Environment>,
    Extension(group_invite_storage): Extension<Arc<GroupInviteStorage>>,
    Extension(group_join_request_storage): Extension<Arc<GroupJoinRequestStorage>>,
) -> Result<Json<GroupInviteResponse>, AppError> {
    let Some(group_invite) = group_invite_storage.get_one(&id).await? else {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "invite_not_found",
            "Group invite not found",
            false,
        ));
    };

    let response = GroupInviteResponse::new(
        group_invite,
        group_join_request_storage,
        &env.invite_link_base_url(),
    )
    .await?;

    Ok(Json(response))
}

/// Delete a group invite
///
/// Deletes a group invite, preventing it from being used further.
/// Only the creator of the invite can delete it.
///
/// # Arguments
///
/// * `user` - The authenticated user attempting to delete the invite
/// * `id` - Path parameter containing the invite ID to delete
///
/// # Returns
///
/// Returns `204 NO_CONTENT` on successful deletion
///
/// # Errors
///
/// Returns an error if:
/// - `404 NOT_FOUND` - Invite with the given ID does not exist
/// - `403 FORBIDDEN` - User is not the creator of the invite
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
/// - `503 SERVICE_UNAVAILABLE` - Database connectivity issues
pub async fn delete_group_invite(
    user: AuthenticatedUser,
    Path(id): Path<String>,
    Extension(group_invite_storage): Extension<Arc<GroupInviteStorage>>,
    Extension(group_join_request_storage): Extension<Arc<GroupJoinRequestStorage>>,
) -> Result<StatusCode, AppError> {
    if id.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_id",
            "ID cannot be empty",
            false,
        ));
    }

    let Some(group_invite) = group_invite_storage.get_one(&id).await? else {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "invite_not_found",
            "Group invite not found",
            false,
        ));
    };

    if group_invite.creator_encrypted_push_id != user.encrypted_push_id {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "You do not have permission to delete this group invite",
            false,
        ));
    }

    // Delete associated join requests first, then delete the invite
    // to maintain referential integrity
    group_join_request_storage
        .delete_by_group_invite_id(&group_invite.id)
        .await?;
    group_invite_storage.delete(&group_invite.id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Retrieves the latest group invite for a given topic created by the authenticated user
///
/// # Arguments
///
/// * `user` - The authenticated user requesting the invite
/// * `query` - Query parameters containing the topic
///
/// # Returns
///
/// Returns `200 OK` with the latest invite on success
///
/// # Errors
///
/// Returns an error if:
/// - `400 BAD_REQUEST` - Invalid query parameters
/// - `404 NOT_FOUND` - No invite found for the given topic
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
/// - `503 SERVICE_UNAVAILABLE` - Database connectivity issues
pub async fn get_latest_group_invite_by_topic(
    user: AuthenticatedUser,
    Extension(env): Extension<Environment>,
    Extension(group_invite_storage): Extension<Arc<GroupInviteStorage>>,
    Extension(group_join_request_storage): Extension<Arc<GroupJoinRequestStorage>>,
    Query(query): Query<GetGroupInvitesByTopicQuery>,
) -> Result<Json<GroupInviteResponse>, AppError> {
    let group_invite = group_invite_storage
        .get_latest_by_topic(&user.encrypted_push_id, &query.topic)
        .await?;

    let Some(group_invite) = group_invite else {
        return Err(AppError::new(
            StatusCode::NOT_FOUND,
            "invite_not_found",
            "Group invite not found",
            false,
        ));
    };

    let response = GroupInviteResponse::new(
        group_invite,
        group_join_request_storage,
        &env.invite_link_base_url(),
    )
    .await?;

    Ok(Json(response))
}
