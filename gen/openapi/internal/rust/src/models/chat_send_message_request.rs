/*
 * Rivet API
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 0.0.1
 * 
 * Generated by: https://openapi-generator.tech
 */




#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct ChatSendMessageRequest {
    #[serde(rename = "message_body")]
    pub message_body: Box<crate::models::ChatSendMessageBody>,
    #[serde(rename = "topic")]
    pub topic: Box<crate::models::ChatSendTopic>,
}

impl ChatSendMessageRequest {
    pub fn new(message_body: crate::models::ChatSendMessageBody, topic: crate::models::ChatSendTopic) -> ChatSendMessageRequest {
        ChatSendMessageRequest {
            message_body: Box::new(message_body),
            topic: Box::new(topic),
        }
    }
}


