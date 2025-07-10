use aws_sdk_s3::{error::SdkError, operation::head_object::HeadObjectError, Client as S3Client};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};

/// Upload data to S3 using presigned URL
pub async fn upload_to_s3(
    presigned_url: &str,
    data: &[u8],
    content_type: Option<&str>,
    checksum_sha256: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    let headers = create_upload_headers(
        data.len(),
        content_type.unwrap_or("application/octet-stream"),
        checksum_sha256,
    );

    let client = reqwest::Client::new();
    client
        .put(presigned_url)
        .headers(headers)
        .body(data.to_vec())
        .send()
        .await
}

/// Download data from S3 using S3 client
pub async fn download_from_s3(
    s3_client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let response = s3_client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let body = response.body.collect().await?;
    Ok(body.into_bytes().to_vec())
}

/// Check if S3 object exists
pub async fn s3_object_exists(
    s3_client: &S3Client,
    bucket: &str,
    key: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    match s3_client.head_object().bucket(bucket).key(key).send().await {
        Ok(_) => Ok(true),
        Err(SdkError::ServiceError(service_err))
            if matches!(service_err.err(), HeadObjectError::NotFound(_)) =>
        {
            Ok(false)
        }
        Err(e) => Err(e.into()),
    }
}

/// Create headers for S3 upload with specific content type and checksum
pub fn create_upload_headers(
    content_length: usize,
    content_type: &str,
    checksum_sha256: Option<&str>,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from(content_length));
    headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());

    if let Some(checksum) = checksum_sha256 {
        headers.insert(
            "x-amz-checksum-sha256",
            HeaderValue::from_str(checksum).unwrap(),
        );
        headers.insert(
            "x-amz-sdk-checksum-algorithm",
            HeaderValue::from_str("SHA256").unwrap(),
        );
    }

    headers
}
