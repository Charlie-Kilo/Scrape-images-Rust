use std::error::Error;
use std::fs;
use std::io::Read;
use rusoto_core::{Region, HttpClient};
use rusoto_credential::ProfileProvider;
use rusoto_s3::{PutObjectRequest, S3Client, S3, ListObjectsV2Request};

// Function to upload a file to an S3 bucket
async fn upload_file_to_s3(bucket_name: &str, file_path: &str) -> Result<(), Box<dyn Error>> {
    // Initialize AWS credentials from profile
    let profile_provider = ProfileProvider::new()?;
    let region = Region::default(); // Change region if needed
    let http_client = HttpClient::new()?;
    let s3_client = S3Client::new_with(http_client, profile_provider, region);

    // Check existing files in the bucket to determine the next available filename
    let request = ListObjectsV2Request {
        bucket: bucket_name.to_owned(),
        prefix: Some("images/".to_owned()),
        ..Default::default()
    };
    let existing_objects = s3_client.list_objects_v2(request).await?; // Now this should work
    let mut existing_files_count = existing_objects.contents.unwrap_or_default().len();

    // Iterate over each file in the directory
    for entry in fs::read_dir(file_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            // Read file content
            let mut file = std::fs::File::open(&path)?;
            let mut file_content = Vec::new();
            file.read_to_end(&mut file_content)?;
            // Determine the next available filename
            let next_filename = format!("images/image{}.jpg", existing_files_count);
            // Prepare request
            let request = PutObjectRequest {
                bucket: bucket_name.to_owned(),
                key: next_filename.clone(), // Use the same filename for all files
                body: Some(file_content.into()),
                ..Default::default()
            };
            // Upload file to S3
            let response = s3_client.put_object(request).await?; // Now this should work
            println!("File uploaded successfully. ETag: {:?}", response.e_tag);
            existing_files_count += 1; // Increment the count for the next file
        }
    }

    Ok(())
}

// Function to remove local files after uploading to S3
fn remove_local_files() -> Result<(), Box<dyn Error>> {
    // Specify the directory path
    let dir_path = "./files";
    // Read the directory
    let paths = fs::read_dir(dir_path)?;
    // Iterate over the directory entries
    for path in paths {
        let file_path = path?.path();
        // Remove the file
        fs::remove_file(&file_path)?;
        println!("Removed local file: {:?}", file_path);
    }
    Ok(())
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set your S3 bucket name
    let bucket_name = "team-3-project-3";
    // Set the directory path containing images to be uploaded
    let directory_path = "files";
    // Upload all files in the directory to S3
    upload_file_to_s3(bucket_name, directory_path).await?;
    // Remove local files after uploading to S3
    remove_local_files()?;
    Ok(())
}
