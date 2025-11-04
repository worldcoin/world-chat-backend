use axum::{extract::Path, http::StatusCode, Json};
use axum_valid::Valid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{middleware::AuthenticatedUser, types::AppError};

// TODO: Remove this once we import storage and implement real logic
/// Status of a group join request
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JoinRequestStatus {
    /// Request is being processed
    InProgress,
    /// Notification has been sent to group members
    NotificationSent,
    /// Request has been approved
    Approved,
    /// Request has been rejected
    Rejected,
}

/// Request to create a new group join request
#[derive(Debug, Deserialize, Serialize, JsonSchema, Validate)]
#[serde(deny_unknown_fields)]
pub struct CreateJoinRequestRequest {
    /// XMTP inbox ID of the requester
    #[validate(length(min = 1))]
    pub inbox_id: String,

    /// ID of the group invite to join
    #[validate(length(min = 1))]
    pub invite_id: String,
}

/// Response when creating a group join request
#[derive(Debug, Serialize, JsonSchema)]
pub struct CreateJoinRequestResponse {
    /// Unique ID of the join request
    pub id: String,

    /// Current status of the join request
    pub status: JoinRequestStatus,
}

/// Response when getting a group join request
#[derive(Debug, Serialize, JsonSchema)]
pub struct GetJoinRequestResponse {
    /// Current status of the join request
    pub status: JoinRequestStatus,
}

/// Create a new group join request
///
/// Creates a request to join a group using an invite. The request will be
/// processed and notifications sent to existing group members for approval.
///
/// # Arguments
///
/// * `user` - The authenticated user creating the join request
/// * `payload` - Request containing inbox ID and invite ID
///
/// # Returns
///
/// Returns `201 CREATED` with the created join request details on success
///
/// # Errors
///
/// Returns an error if:
/// - `400 BAD_REQUEST` - Invalid request parameters
/// - `404 NOT_FOUND` - Invite ID does not exist
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `409 CONFLICT` - User has already requested to join this group
/// - `500 INTERNAL_SERVER_ERROR` - Processing error
pub async fn create_join_request(
    _user: AuthenticatedUser,
    Valid(Json(_payload)): Valid<Json<CreateJoinRequestRequest>>,
) -> Result<(StatusCode, Json<CreateJoinRequestResponse>), AppError> {
    // TODO: Implement actual business logic
    // For now, return a mock response

    let mock_response = CreateJoinRequestResponse {
        id: "jr-test-id".to_string(),
        status: JoinRequestStatus::InProgress,
    };

    Ok((StatusCode::CREATED, Json(mock_response)))
}

/// Get a group join request by ID
///
/// Retrieves the current status of a group join request.
///
/// # Arguments
///
/// * `user` - The authenticated user requesting the status
/// * `id` - Path parameter containing the join request ID
///
/// # Returns
///
/// Returns `200 OK` with the join request status on success
///
/// # Errors
///
/// Returns an error if:
/// - `404 NOT_FOUND` - Join request with the given ID does not exist
/// - `403 FORBIDDEN` - User is not authorized to view this join request
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Storage operation fails
pub async fn get_join_request(
    _user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<GetJoinRequestResponse>, AppError> {
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

    let mock_response = GetJoinRequestResponse {
        status: JoinRequestStatus::NotificationSent,
    };

    Ok(Json(mock_response))
}

/// Update a group join request
///
/// Updates the status of a group join request. Currently only supports
/// approving requests. Must be called by a group member with appropriate permissions.
///
/// # Arguments
///
/// * `user` - The authenticated user updating the join request
/// * `id` - Path parameter containing the join request ID
/// * `payload` - Request containing the new status
///
/// # Returns
///
/// Returns `204 NO_CONTENT`
///
/// # Errors
///
/// Returns an error if:
/// - `404 NOT_FOUND` - Join request with the given ID does not exist
/// - `403 FORBIDDEN` - User is not authorized to update this join request
/// - `400 BAD_REQUEST` - Invalid status transition
/// - `401 UNAUTHORIZED` - Invalid or missing authentication
/// - `500 INTERNAL_SERVER_ERROR` - Update operation fails
pub async fn approve_join_request(
    _user: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
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

    Ok(StatusCode::NO_CONTENT)
}
