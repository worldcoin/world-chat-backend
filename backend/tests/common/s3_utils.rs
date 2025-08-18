use reqwest::header::{HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};

/// Upload data to S3 using presigned URL
pub async fn upload_to_s3(
    presigned_url: &str,
    data: &[u8],
    content_type: &str,
    checksum_sha256: &str,
) -> Result<reqwest::Response, reqwest::Error> {
    let headers = create_upload_headers(data.len(), content_type, checksum_sha256);

    let client = reqwest::Client::new();
    client
        .put(presigned_url)
        .headers(headers)
        .body(data.to_vec())
        .send()
        .await
}

/// Download data from asset URL using HTTP
pub async fn download_from_asset_url(
    asset_url: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.get(asset_url).send().await?;

    if response.status().is_success() {
        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    } else {
        Err(format!(
            "Failed to download from {}: HTTP {}",
            asset_url,
            response.status()
        )
        .into())
    }
}

/// Check if asset exists at URL using HTTP HEAD request
pub async fn asset_exists_at_url(asset_url: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client.head(asset_url).send().await?;

    Ok(response.status().is_success())
}

/// Create headers for S3 upload with specific content type and checksum
pub fn create_upload_headers(
    content_length: usize,
    content_type: &str,
    checksum_sha256: &str,
) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_LENGTH, HeaderValue::from(content_length));
    headers.insert(CONTENT_TYPE, HeaderValue::from_str(content_type).unwrap());
    headers.insert(
        "x-amz-checksum-sha256",
        HeaderValue::from_str(checksum_sha256).unwrap(),
    );
    headers.insert(
        "x-amz-sdk-checksum-algorithm",
        HeaderValue::from_str("SHA256").unwrap(),
    );

    headers
}
