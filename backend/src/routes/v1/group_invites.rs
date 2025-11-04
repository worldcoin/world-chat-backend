use axum::{
    extract::{Path, Query},
    http::StatusCode,
    Json,
};
use axum_valid::Valid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{middleware::AuthenticatedUser, types::AppError};

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

    /// Creator's encrypted push ID
    #[validate(length(min = 1))]
    pub creator_encrypted_push_id: String,

    /// Maximum number of uses for this invite (optional)
    pub max_uses: Option<i32>,

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
pub async fn create_group_invite(
    _user: AuthenticatedUser,
    Valid(Json(_payload)): Valid<Json<CreateGroupInviteRequest>>,
) -> Result<Json<GroupInviteResponse>, AppError> {
    // TODO: Implement actual business logic
    // For now, return a mock response

    let mock_response = GroupInviteResponse {
        id: "test-invite-id".to_string(),
        group_name: "Mock Group".to_string(),
        link_url: "https://example.com/invite/test-invite-id".to_string(),
    };

    Ok(Json(mock_response))
}

/// Get a group invite by ID
///
/// Retrieves details about a specific group invite including its current usage
/// and validity status.
///
/// # Arguments
///
/// * `user` - The authenticated user requesting the invite
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
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
pub async fn get_group_invite(
    _user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<GroupInviteResponse>, AppError> {
    // TODO: Implement actual business logic
    // For now, return a mock response

    if id.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_id",
            "ID cannot be empty",
            false,
        ));
    }

    let mock_response = GroupInviteResponse {
        id: "test-invite-id".to_string(),
        group_name: "Mock Group".to_string(),
        link_url: "https://example.com/invite/test-invite-id".to_string(),
    };

    Ok(Json(mock_response))
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
pub async fn delete_group_invite(
    _user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    // TODO: Implement actual business logic
    // For now, just validate and return success

    if id.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_id",
            "ID cannot be empty",
            false,
        ));
    }

    // Mock: Simulate checking if invite exists and user is creator
    // In real implementation:
    // 1. Fetch the invite by ID
    // 2. Check if it exists (404 if not)
    // 3. Check if user.encrypted_push_id matches creator_encrypted_push_id (403 if not)
    // 4. Delete the invite
    // 5. Delete all associated join requests

    Ok(StatusCode::NO_CONTENT)
}

/// Get group invites by topic
///
/// Retrieves all group invites for a specific topic.
///
/// # Arguments
///
/// * `user` - The authenticated user requesting the invites
/// * `query` - Query parameters containing the topic
///
/// # Returns
///
/// Returns `200 OK` with a list of invites on success
///
/// # Errors
///
/// Returns an error if:
/// - `400 BAD_REQUEST` - Invalid query parameters
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
pub async fn get_group_invites_by_topic(
    _user: AuthenticatedUser,
    Query(query): Query<GetGroupInvitesByTopicQuery>,
) -> Result<Json<Vec<GroupInviteResponse>>, AppError> {
    // TODO: Implement actual business logic
    // For now, return a mock response

    if query.topic.is_empty() {
        return Err(AppError::new(
            StatusCode::BAD_REQUEST,
            "invalid_topic",
            "Topic cannot be empty",
            false,
        ));
    }

    // Mock: Return a list of mock invites
    let mock_invites = vec![GroupInviteResponse {
        id: "test-invite-id".to_string(),
        group_name: "Mock Group".to_string(),
        link_url: "https://example.com/invite/test-invite-id".to_string(),
    }];

    Ok(Json(mock_invites))
}
