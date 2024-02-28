use std::{
    error::Error,
    process::Command,
    path::Path,
    fmt,
    fs,
    sync::Arc,
};
use scraper::{Selector, Html};
use serde_json::Value;
use tokio::sync::Semaphore;
use warp::{Filter, http::header};

#[derive(Debug)]
struct CustomError(String);

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for CustomError {}

#[derive(Debug)]
struct ErrorRejection(Box<dyn Error + Send + Sync>);

impl warp::reject::Reject for ErrorRejection {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Set your S3 bucket name
    let _bucket_name = "team-3-project-3";
    // Define the semaphore with a limit on the maximum number of concurrent tasks
    let semaphore = Arc::new(Semaphore::new(2)); // Adjust the limit as needed
    // Define the endpoint filter to handle POST requests with JSON body
    let post_url = warp::path("url")
        .and(warp::post())
        .and(warp::header::<String>("requestId"))
        .and(warp::body::json())
        .and_then(move |request_id: String, body: Value| {
            let semaphore = Arc::clone(&semaphore);
            async move {
                // Acquire a semaphore permit before processing the URL
                let permit = semaphore.acquire().await.unwrap();
                // Extract the URL from the JSON body
                let url = body["url"].as_str().unwrap_or_default();
                // Debug print to see if the Warp server received the POST request
                println!("Received POST request with URL: {}", url);
                // Call the main function with the received URL and request ID
                match process_url(url, &request_id).await {
                    Ok(_) => {
                        // Release the semaphore permit after processing the URL
                        drop(permit);
                        Ok(warp::reply::html("Received URL successfully"))
                    },
                    Err(e) => {
                        // Release the semaphore permit in case of error
                        drop(permit);
                        eprintln!("Error processing URL: {}", e);
                        // Define a custom rejection type to wrap errors
                        Err(warp::reject::custom(ErrorRejection(Box::new(CustomError(format!("{}", e))))))
                    }
                }
            }
        });
    // Combine all routes
    let routes = post_url.with(warp::log("image_save"));
    // Start the warp server
    warp::serve(routes)
        .run(([127, 0, 0, 1], 3032))
        .await;
    Ok(())
}

async fn process_url(url: &str, request_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Clone the URL and request ID to ensure they're owned by the closure
    let url = url.to_string();
    let request_id = request_id.to_string();
    // Create a directory for each request ID if it doesn't exist
    let dir_path = format!("./files/{}", request_id);
    if !Path::new(&dir_path).exists() {
        fs::create_dir_all(&dir_path)?;
    }
    // Execute the blocking operations within a separate blocking context
    let result = tokio::task::spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
        // Extract the auction ID based on the URL format
        let auction_id = if url.contains("fromjapan.co") {
            // Extract the ID from the URL containing "fromjapan.co"
            extract_auction_id_from_fromjapan(&url)? // Change _url to url here
        } else {
            // Extract the ID using a different method for other URLs
            extract_auction_id_from_other(&url)? // Change _url to url here
        };
        // Construct the URL for the Yahoo Auctions page
        let yahoo_url = format!("https://page.auctions.yahoo.co.jp/jp/auction/{}", auction_id);
        // Fetch HTML content from the new URL
        let body = reqwest::blocking::get(&yahoo_url)?.text()?;
        // Parse HTML using the scraper library
        let document = scraper::Html::parse_document(&body);
        // Extract JSON data from the HTML
        let json_data = extract_json_data(&document)?;
        // Extract image URLs from the JSON data and save the images locally
        extract_image_urls_from_json(&json_data, &dir_path)?;
        // Print the request ID and URL for debugging
        println!("Request ID: {}", request_id);
        println!("URL: {}", url);
        // Upload files to S3 using s3.exe
        let output = Command::new("s3.exe")
            .args(&[&request_id]) // Pass the request ID as an argument to s3.exe
            .output()?;
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("{}", String::from_utf8_lossy(&output.stderr));
        // Debugging line to print success response with URL
        println!("Success response sent to API Gateway for request ID {} with URL {}", request_id, url);
        Ok(())
    }).await;
    result?
}


// Function to extract auction ID from URL containing "fromjapan.co"
fn extract_auction_id_from_fromjapan(url: &str) -> Result<&str, &'static str> {
    if let Some(input_index) = url.find("/input/") {
        // Find the position of "/input/" in the URL
        let id_start_index = input_index + "/input/".len();
        // Extract the auction ID that follows "/input/"
        let id_with_slash = &url[id_start_index..];
        // Remove any trailing slashes
        Ok(id_with_slash.trim_end_matches('/'))
    } else {
        Err("Auction ID not found in the URL")
    }
}

// Function to extract auction ID from other URLs
fn extract_auction_id_from_other(url: &str) -> Result<&str, &'static str> {
    // Extract the ID based on the URL format for other cases
    // You need to implement this based on the URL structure for other websites
    unimplemented!("Function to extract auction ID from other URLs")
}

// Function to extract JSON data from the HTML document
fn extract_json_data(document: &Html) -> Result<Value, Box<dyn Error + Send + Sync>> {
    // Define the CSS selector for the script containing JSON data
    let selector = Selector::parse("script#__NEXT_DATA__").unwrap();
    // Find the script element containing JSON data
    let script_element = document.select(&selector).next().ok_or("Script element not found")?;
    // Extract the JSON data from the script element
    let json_text = script_element.text().collect::<String>();
    // Parse the JSON data
    let json_data: Value = serde_json::from_str(&json_text)?;
    Ok(json_data)
}

fn extract_image_urls_from_json(json_data: &Value, dir_path: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Check if the JSON data contains the expected structure
    if let Some(img_array) = json_data["props"]["pageProps"]["initialState"]["itempage"]["item"]["item"]["img"].as_array() {
        // Iterate over each image object in the array
        for (index, img) in img_array.iter().enumerate() {
            // Check if the image object has the "image" field containing the URL
            if let Some(image_url) = img["image"].as_str() {
                // Generate the file name
                let file_name = format!("image{}.jpg", index + 1);
                // Download the image and save it to the files directory
                let mut response = reqwest::blocking::get(image_url)?;
                let mut file = fs::File::create(format!("{}/{}", dir_path, file_name))?;
                response.copy_to(&mut file)?;
                println!("Image {} downloaded and saved", index + 1);
            }
        }
        return Ok(());
    }
    println!("Expected JSON structure not found");
    Ok(())
}
