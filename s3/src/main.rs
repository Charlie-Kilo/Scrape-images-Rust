use std::{error::Error, path::PathBuf, io::Read, fs, env};
use rusoto_core::{Region, HttpClient};
use rusoto_credential::ProfileProvider;
use rusoto_s3::{PutObjectRequest, S3Client, S3, ListObjectsV2Request};
use reqwest::Client;

async fn upload_file_to_s3(bucket_name: &str, file_path: &str, request_id: &str) -> Result<usize, Box<dyn Error>> {
    // Print the request ID for debugging
    println!("Request ID: {}", request_id);
    
    // Initialize AWS credentials from profile
    let profile_provider = ProfileProvider::new()?;
    let region = Region::default();
    let http_client = HttpClient::new()?;
    let s3_client = S3Client::new_with(http_client, profile_provider, region);
    // Determine the latest folder number in the S3 bucket
    let latest_folder_number = get_latest_folder_number(bucket_name, &s3_client).await?;
    println!("Latest folder number: {}", latest_folder_number);
    // Increment the folder number for the new upload
    let new_folder_number = latest_folder_number + 1;
    println!("New folder number: {}", new_folder_number);
    // Initialize a counter for uploaded files
    let mut counter = 0;
    // Construct the directory path with the request ID
    let dir_path_with_request_id = format!("{}/{}", file_path, request_id);
    println!("Directory path with request ID: {}", dir_path_with_request_id);
    // Iterate over each file in the directory
    for entry in fs::read_dir(&dir_path_with_request_id)? {
        let entry = entry?;
        let path = entry.path();
        println!("Processing file: {:?}", path);
        if path.is_file() {
            // Print the local file path being uploaded from
            println!("Uploading file from: {:?}", path);
            // Read file content
            let mut file = std::fs::File::open(&path)?;
            let mut file_content = Vec::new();
            file.read_to_end(&mut file_content)?;
            // Determine the next available filename using the counter
            let next_filename = format!("images/{}/image{}.jpg", new_folder_number, counter);
            println!("Next filename: {}", next_filename);
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
    Ok(counter) // Return the counter value
}

// Function to remove local files after uploading to S3
fn remove_local_files(request_id: &str) -> Result<(), Box<dyn Error>> {
    let dir_path = PathBuf::from(format!("./files/{}", request_id));
    println!("Removing local files from directory: {:?}", dir_path);
    let paths = fs::read_dir(&dir_path)?;
    for path in paths {
        let file_path = path?.path();
        println!("Removing local file: {:?}", file_path);
        if let Err(err) = fs::remove_file(&file_path) {
            println!("Error removing local file: {:?}", err);
        } else {
            println!("Removed local file: {:?}", file_path);
        }
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

async fn send_file_path_to_api_gateway(bucket_name: &str, new_folder_number: usize, counter: usize, request_id: &str) -> Result<(), Box<dyn Error>> {
    // Construct the URL of your API gateway
    let api_url = "http://localhost:3031/path"; // Change this URL to match your API endpoint
    // Construct the file name using the counter
    let final_image_path = format!("s3://{}/images/{}/image{}.jpg", bucket_name, new_folder_number, counter);
    // Create a reqwest Client instance
    let client = reqwest::Client::new();
    // Create a JSON object with the file path
    let json_body = serde_json::json!({
        "final_image_path": final_image_path,
        "requestId": request_id // Include the request ID in the JSON body
    });
    // Send a POST request to the API gateway with the request ID in the headers
    let response = client.post(api_url)
        .json(&json_body)
        .header("requestId", request_id) // Include the request ID in the headers
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
    // Retrieve the request ID from command-line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <request_id>", args[0]);
        std::process::exit(1);
    }
    let request_id = &args[1];

    let bucket_name = "team-3-project-3";
    let directory_path = "files";
    // Upload files to S3 and get the counter value
    let counter = upload_file_to_s3(bucket_name, directory_path, request_id).await?;
    // Get the latest folder number in the S3 bucket
    let profile_provider = ProfileProvider::new()?;
    let region = Region::default();
    let http_client = HttpClient::new()?;
    let s3_client = S3Client::new_with(http_client, profile_provider, region);
    let last_folder_number = get_latest_folder_number(bucket_name, &s3_client).await?;
    send_file_path_to_api_gateway(bucket_name, last_folder_number, counter-1, request_id).await?;
    // Remove local files after uploading to S3
    remove_local_files(request_id)?;
    Ok(())
}
