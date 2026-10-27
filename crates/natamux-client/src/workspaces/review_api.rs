//! The review comment routes of the Changes tab (`/tasks/:id/review-comments`).

use hyper::Method;
use natamux_proto::terminals::query::encode;
use natamux_proto::workspaces::review_comments::{
    NewReviewComment, ReviewComment, ReviewCommentAnswer, ReviewCommentList, ReviewCommentPatch,
    SendReviewAnswer,
};

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with, send};

fn base(task_id: &str) -> String {
    format!("/tasks/{}/review-comments", encode(task_id))
}

/// Every comment of the task, where its code is now.
pub async fn list(config: &Config, task_id: &str) -> Result<Vec<ReviewComment>, ClientError> {
    let list: ReviewCommentList = call(config, Method::GET, &base(task_id)).await?;
    Ok(list.comments)
}

pub async fn add(
    config: &Config,
    task_id: &str,
    input: &NewReviewComment,
) -> Result<ReviewComment, ClientError> {
    let answer: ReviewCommentAnswer =
        call_with(config, Method::POST, &base(task_id), input).await?;
    Ok(answer.comment)
}

pub async fn update(
    config: &Config,
    task_id: &str,
    id: &str,
    patch: &ReviewCommentPatch,
) -> Result<ReviewComment, ClientError> {
    let path = format!("{}/{}", base(task_id), encode(id));
    let answer: ReviewCommentAnswer = call_with(config, Method::PATCH, &path, patch).await?;
    Ok(answer.comment)
}

pub async fn delete(config: &Config, task_id: &str, id: &str) -> Result<(), ClientError> {
    let path = format!("{}/{}", base(task_id), encode(id));
    send(config, Method::DELETE, &path, None).await.map(|_| ())
}

/// Every draft to the agent in one prompt.
pub async fn send_all(config: &Config, task_id: &str) -> Result<SendReviewAnswer, ClientError> {
    let path = format!("{}/send", base(task_id));
    call(config, Method::POST, &path).await
}
