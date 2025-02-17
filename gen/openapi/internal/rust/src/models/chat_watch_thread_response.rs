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
pub struct ChatWatchThreadResponse {
    /// All messages new messages posted to this thread. Ordered old to new.
    #[serde(rename = "chat_messages")]
    pub chat_messages: Vec<crate::models::ChatMessage>,
    /// All identities that are currently typing in this thread.
    #[serde(rename = "typing_statuses", skip_serializing_if = "Option::is_none")]
    pub typing_statuses: Option<Vec<crate::models::ChatIdentityTypingStatus>>,
    #[serde(rename = "watch")]
    pub watch: Box<crate::models::WatchResponse>,
}

impl ChatWatchThreadResponse {
    pub fn new(chat_messages: Vec<crate::models::ChatMessage>, watch: crate::models::WatchResponse) -> ChatWatchThreadResponse {
        ChatWatchThreadResponse {
            chat_messages,
            typing_statuses: None,
            watch: Box::new(watch),
        }
    }
}


