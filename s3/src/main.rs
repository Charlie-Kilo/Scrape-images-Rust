use std::{error::Error, fs, io::Read};
use rusoto_core::{Region, HttpClient};
use rusoto_credential::ProfileProvider;
use rusoto_s3::{PutObjectRequest, S3Client, S3, ListObjectsV2Request};
use reqwest::Client;

async fn upload_file_to_s3(bucket_name: &str, file_path: &str) -> Result<(), Box<dyn Error>> {
    // Initialize AWS credentials from profile
    let profile_provider = ProfileProvider::new()?;
    let region = Region::default();
    let http_client = HttpClient::new()?;
    let s3_client = S3Client::new_with(http_client, profile_provider, region);

    // Determine the latest folder number in the S3 bucket
    let latest_folder_number = get_latest_folder_number(bucket_name, &s3_client).await?;

    // Increment the folder number for the new upload
    let new_folder_number = latest_folder_number + 1;

    // Initialize a counter for uploaded files
    let mut counter = 0;

    // Iterate over each file in the directory
    for entry in fs::read_dir(file_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            // Read file content
            let mut file = std::fs::File::open(&path)?;
            let mut file_content = Vec::new();
            file.read_to_end(&mut file_content)?;

            // Determine the next available filename using the counter
            let next_filename = format!("images/{}/image{}.jpg", new_folder_number, counter);

            // Prepare request
            let request = PutObjectRequest {
                bucket: bucket_name.to_owned(),
                key: next_filename.clone(),
                body: Some(file_content.into()),
                ..Default::default()
            };

            // Upload file to S3
            let _response = s3_client.put_object(request).await?;
            println!("File uploaded successfully: {}", next_filename);

            // Increment the counter
            counter += 1;
        }
    }

    Ok(())
}

// Function to remove local files after uploading to S3
fn remove_local_files() -> Result<(), Box<dyn Error>> {
    let dir_path = "./files";
    let paths = fs::read_dir(dir_path)?;
    for path in paths {
        let file_path = path?.path();
        fs::remove_file(&file_path)?;
        println!("Removed local file: {:?}", file_path);
    }
    Ok(())
}

async fn get_latest_folder_number(bucket_name: &str, s3_client: &S3Client) -> Result<usize, Box<dyn Error>> {
    let request = ListObjectsV2Request {
        bucket: bucket_name.to_owned(),
        prefix: Some("images/".to_string()),
        delimiter: Some("/".to_string()),
        ..Default::default()
    };
    let result = s3_client.list_objects_v2(request).await?;
    let folders = result.common_prefixes.unwrap_or_default();

    let mut folder_numbers: Vec<usize> = folders
        .into_iter()
        .filter_map(|folder| {
            let prefix = folder.prefix?;
            prefix.trim_end_matches('/').rsplit('/').next().and_then(|folder_name| folder_name.parse().ok())
        })
        .collect();

    folder_numbers.sort_unstable_by(|a, b| b.cmp(a));

    Ok(folder_numbers.into_iter().next().unwrap_or(0))
}

async fn send_file_path_to_api_gateway(bucket_name: &str, new_folder_number: usize, counter: usize) -> Result<(), Box<dyn Error>> {
    // Construct the URL of your API gateway
    let api_url = "http://localhost:3031/path"; // Change this URL to match your API endpoint

    // Construct the file name using the counter
    let final_image_path = format!("s3://{}/images/{}/image{}.jpg", bucket_name, new_folder_number, counter);

    // Create a reqwest Client instance
    let client = reqwest::Client::new();
    // Create a JSON object with the file path
    let json_body = serde_json::json!({
        "final_image_path": final_image_path
    });
    // Send a POST request to the API gateway
    let response = client.post(api_url)
        .json(&json_body)
        .send()
        .await?;
    // Check if the request was successful
    if response.status().is_success() {
        println!("File path sent to API gateway successfully");
    } else {
        println!("Failed to send file path to API gateway: {}", response.status());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let bucket_name = "team-3-project-3";
    let directory_path = "files";
    // Upload files to S3
    upload_file_to_s3(bucket_name, directory_path).await?;
    // Get the latest folder number in the S3 bucket
    let profile_provider = ProfileProvider::new()?;
    let region = Region::default();
    let http_client = HttpClient::new()?;
    let s3_client = S3Client::new_with(http_client, profile_provider, region);
    let last_folder_number = get_latest_folder_number(bucket_name, &s3_client).await?;
    // Call send_file_path_to_api_gateway with the latest folder number and the counter
    send_file_path_to_api_gateway(bucket_name, last_folder_number, 0).await?;
    // Remove local files after uploading to S3
    remove_local_files()?;
    Ok(())
}
