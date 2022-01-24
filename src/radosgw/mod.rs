pub mod awscredentials;
pub mod uploader;

use rusoto_core::{ByteStream, RusotoError};
use rusoto_s3::{
    AbortMultipartUploadError, AbortMultipartUploadOutput, AbortMultipartUploadRequest,
    CompleteMultipartUploadError, CompleteMultipartUploadOutput, CompleteMultipartUploadRequest,
    CompletedMultipartUpload, CompletedPart, CreateMultipartUploadError,
    CreateMultipartUploadOutput, CreateMultipartUploadRequest, ListObjectsV2Error,
    ListObjectsV2Request, PutObjectError, PutObjectOutput, PutObjectRequest, S3Client,
    UploadPartError, UploadPartOutput, UploadPartRequest, S3,
};

use crate::riakcs::dto::ObjectMetadataResponse;

#[derive(Debug, Clone)]
pub struct RadosGW {
    endpoint: String,
    access_key: String,
    secret_key: String,
    bucket: String,
}

impl RadosGW {
    pub fn new(
        endpoint: String,
        access_key: String,
        secret_key: String,
        bucket: String,
    ) -> RadosGW {
        RadosGW {
            endpoint,
            access_key,
            secret_key,
            bucket,
        }
    }

    fn get_client(&self) -> S3Client {
        let radosgw_credential_provider = awscredentials::AWSCredentialsProvider::new(
            self.access_key.clone(),
            self.secret_key.clone(),
        );
        let http_client = rusoto_core::HttpClient::new().unwrap();

        S3Client::new_with(
            http_client,
            radosgw_credential_provider,
            rusoto_core::Region::Custom {
                name: "RadosGW".to_string(),
                endpoint: self.endpoint.clone(),
            },
        )
    }

    pub async fn put_object(
        &self,
        key: String,
        object_metadata: &ObjectMetadataResponse,
        size: i64,
        body: ByteStream,
    ) -> Result<PutObjectOutput, RusotoError<PutObjectError>> {
        let put_object_request = PutObjectRequest {
            body: Some(body),
            key,
            bucket: self.bucket.clone(),
            content_length: Some(size),
            acl: if object_metadata.acl_public {
                Some("public-read".to_string())
            } else {
                None
            },
            cache_control: object_metadata.metadata.cache_control.clone(),
            content_disposition: object_metadata.metadata.content_disposition.clone(),
            content_encoding: object_metadata.metadata.content_encoding.clone(),
            content_language: object_metadata.metadata.content_language.clone(),
            content_md5: object_metadata.metadata.content_md5.clone(),
            content_type: object_metadata.metadata.content_type.clone(),
            expires: object_metadata.metadata.expires.clone(),
            ..Default::default()
        };

        let client = self.get_client();
        client.put_object(put_object_request).await
    }

    pub async fn create_multipart_upload(
        &self,
        key: String,
        object_metadata: &ObjectMetadataResponse,
    ) -> Result<CreateMultipartUploadOutput, RusotoError<CreateMultipartUploadError>> {
        let multipart_upload_request = CreateMultipartUploadRequest {
            key,
            bucket: self.bucket.clone(),
            acl: if object_metadata.acl_public {
                Some("public-read".to_string())
            } else {
                None
            },
            // We don't have the content_md5 in this list but I don't think we really care
            cache_control: object_metadata.metadata.cache_control.clone(),
            content_disposition: object_metadata.metadata.content_disposition.clone(),
            content_encoding: object_metadata.metadata.content_encoding.clone(),
            content_language: object_metadata.metadata.content_language.clone(),
            content_type: object_metadata.metadata.content_type.clone(),
            expires: object_metadata.metadata.expires.clone(),
            ..Default::default()
        };

        let client = self.get_client();
        client
            .create_multipart_upload(multipart_upload_request)
            .await
    }

    pub async fn put_object_part(
        &self,
        key: String,
        size: i64,
        body: ByteStream,
        upload_id: String,
        part_number: i64,
    ) -> Result<UploadPartOutput, RusotoError<UploadPartError>> {
        let part_upload_request = UploadPartRequest {
            key,
            bucket: self.bucket.clone(),
            body: Some(body),
            upload_id,
            part_number,
            content_length: Some(size),
            ..Default::default()
        };

        let client = self.get_client();
        client.upload_part(part_upload_request).await
    }

    pub async fn complete_multipart_upload(
        &self,
        key: String,
        upload_id: String,
        parts: Vec<(usize, UploadPartOutput)>,
    ) -> Result<CompleteMultipartUploadOutput, RusotoError<CompleteMultipartUploadError>> {
        let completed_multipart_upload_parts = CompletedMultipartUpload {
            parts: Some(
                parts
                    .iter()
                    .map(|(part_number, part)| CompletedPart {
                        e_tag: part.e_tag.clone(),
                        part_number: Some(*part_number as i64),
                    })
                    .collect(),
            ),
        };

        let complete_multipart_upload_request = CompleteMultipartUploadRequest {
            key,
            bucket: self.bucket.clone(),
            multipart_upload: Some(completed_multipart_upload_parts),
            upload_id,
            ..Default::default()
        };

        let client = self.get_client();
        client
            .complete_multipart_upload(complete_multipart_upload_request)
            .await
    }

    pub async fn abort_multipart_upload(
        &self,
        key: String,
        upload_id: String,
    ) -> Result<AbortMultipartUploadOutput, RusotoError<AbortMultipartUploadError>> {
        let abort_multipart_upload_request = AbortMultipartUploadRequest {
            key,
            bucket: self.bucket.clone(),
            upload_id,
            ..Default::default()
        };

        let client = self.get_client();
        client
            .abort_multipart_upload(abort_multipart_upload_request)
            .await
    }

    pub async fn list_objects(
        &self,
    ) -> Result<Vec<rusoto_s3::Object>, RusotoError<ListObjectsV2Error>> {
        let mut results = Vec::new();

        loop {
            let list_objects_request = ListObjectsV2Request {
                bucket: self.bucket.clone(),
                start_after: results.last().map(|obj: &rusoto_s3::Object| {
                    String::from(obj.key.as_ref().expect("Object should have a key"))
                }),
                ..Default::default()
            };

            let client = self.get_client();
            let mut objects = client
                .list_objects_v2(list_objects_request.clone())
                .await
                .map(|res| res.contents.unwrap_or_default())?;

            if objects.is_empty() {
                break;
            }

            results.append(&mut objects);
        }

        Ok(results)
    }
}
