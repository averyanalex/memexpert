use anyhow::Result;
use reqwest::Client;

pub struct AiBox {
    client: Client,
}

impl AiBox {
    pub fn new() -> Self {
        let client = Client::new();
        Self { client }
    }

    pub async fn clip_image(&self, image: Vec<u8>) -> Result<Vec<f32>> {
        let file_part = reqwest::multipart::Part::bytes(image)
            .file_name("image.jpg")
            .mime_str("image/jpeg")?;
        let form = reqwest::multipart::Form::new().part("image", file_part);

        let res = self
            .client
            .post("http://127.0.0.1:8736/clip/image")
            .multipart(form)
            .send()
            .await?
            .error_for_status()?;

        Ok(res.json().await?)
    }

    pub async fn clip_text(&self, text: &str) -> Result<Vec<f32>> {
        let res = self
            .client
            .get("http://127.0.0.1:8736/clip/text")
            .query(&[("text", text)])
            .send()
            .await?
            .error_for_status()?;

        Ok(res.json().await?)
    }

    pub async fn translation(&self, text: &str) -> Result<String> {
        let res = self
            .client
            .get("http://127.0.0.1:8736/translation")
            .query(&[("text", text)])
            .send()
            .await?
            .error_for_status()?;

        Ok(res.json().await?)
    }
}
