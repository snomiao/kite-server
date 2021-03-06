use crate::error::{ApiError, Result};
use crate::models::PageView;
use chrono::{NaiveDateTime, Utc};
use num_traits::ToPrimitive;
use serde::Serialize;
use sqlx::{postgres::PgQueryAs, PgPool};
use thiserror::Error;

#[derive(Debug, Error, ToPrimitive)]
pub enum CheckingError {
    #[error("无审核记录或个人信息填写错误")]
    NoSuchRecord = 1001,
    #[error("需要先实名认证")]
    IdentityNeeded = 1003,
}

impl Into<ApiError> for CheckingError {
    fn into(self) -> ApiError {
        ApiError {
            code: self.to_u16().unwrap(), // Error code
            inner_msg: None,
            error_msg: Some(self.to_string()), // Error message
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Approval {
    /// Serial id.
    pub id: i32,
    /// Student ID
    #[serde(rename = "studentId")]
    pub student_id: String,
    /// Real name
    pub name: String,
    /// Approved time
    #[serde(rename = "approvedTime")]
    pub approved_time: NaiveDateTime,
    /* Belows are some personal information */
    pub college: String,
    pub major: Option<String>,
    #[serde(rename = "certStatus")]
    pub cert_status: Option<bool>,
}

impl Approval {
    /// Create new struct by record id.
    pub fn new(id: i32) -> Self {
        Self {
            id,
            ..Self::default()
        }
    }

    /// Save to database.
    pub async fn submit(&mut self, client: &PgPool) -> Result<()> {
        let approval_id: (i32,) = sqlx::query_as(
            "INSERT INTO checking.approvals (student_id, name, approved_time, college, major)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id",
        )
        .bind(&self.student_id)
        .bind(&self.name)
        .bind(&self.approved_time)
        .bind(&self.college)
        .bind(&self.major)
        .fetch_one(client)
        .await?;

        // Note: BUG: submit function has not fetch self.cert_status, so if someone already set his
        // real identity, when manager add approval record, self.cert_status is always false, try to
        // refresh can solve this problem.
        // 2020.7.16
        self.id = approval_id.0;
        Ok(())
    }

    /// Get personal information and whether he is approved in BY UID
    pub async fn query_by_uid(client: &PgPool, uid: i32) -> Result<Self> {
        let approval_record: Option<Approval> = sqlx::query_as(
            "SELECT id, identities.student_id, name, approved_time, college, major, true AS cert_status
                    FROM public.identities
                LEFT JOIN checking.approvals
                    ON identities.student_id = approvals.student_id
                        AND identities.realname = approvals.name
                WHERE
                    (identities.oa_certified = true
                    OR (identities.identity_number = approvals.identity_number
                        AND length(identities.identity_number) != 0) )
                    AND approved_time is not null
                    AND identities.uid = $1 LIMIT 1",
        )
        .bind(uid)
        .fetch_optional(client)
        .await?;
        approval_record.ok_or(CheckingError::NoSuchRecord.into())
    }

    /// Delete approve record.
    pub async fn delete(self, client: &PgPool) -> Result<()> {
        let _ = sqlx::query("DELETE FROM checking.approvals WHERE id = $1")
            .bind(self.id)
            .execute(client)
            .await?;
        Ok(())
    }

    /// Get Approve List
    pub async fn list(client: &PgPool, college: &Option<String>, page: &PageView) -> Result<Vec<Self>> {
        let approve_list = sqlx::query_as(
            "SELECT id, approvals.student_id, approvals.name, approved_time, college, major,
                    (identities.oa_certified = true
                    OR (identities.identity_number = approvals.identity_number
                        AND length(identities.identity_number) <> 0) ) AS cert_status
                FROM checking.approvals
                LEFT JOIN public.identities
                ON identities.student_id = approvals.student_id
                AND approved_time is not null 
                WHERE college LIKE $1 ORDER BY approved_time DESC 
                OFFSET $2 LIMIT $3",
        )
        .bind(if let Some(college) = college {
            format!("'%{}%'", college)
        } else {
            "%".to_string()
        })
        .bind(page.offset(50) as i32)
        .bind(page.count(50) as i32)
        .fetch_all(client)
        .await?;
        Ok(approve_list)
    }

    /// Search student name
    pub async fn search(client: &PgPool, query_string: &String, count: u16) -> Result<Vec<Self>> {
        let result: Vec<Self> = sqlx::query_as(
            "SELECT a.id, a.student_id, name, approved_time, college, major,
                    (i.oa_certified = true
                    OR (i.identity_number = a.identity_number
                        AND length(i.identity_number) <> 0) ) AS cert_status
                FROM checking.approvals a
                LEFT JOIN public.identities i
                ON a.student_id = i.student_id
                WHERE name like $1 ORDER BY approved_time DESC LIMIT $2",
        )
        .bind(format!("%{}%", query_string))
        .bind(count as i32)
        .fetch_all(client)
        .await?;
        Ok(result)
    }
}

impl Default for Approval {
    fn default() -> Self {
        Approval {
            id: 0,
            student_id: "".to_string(),
            name: "".to_string(),
            approved_time: Utc::now().naive_local(),
            cert_status: Some(false),
            college: "".to_string(),
            major: None,
        }
    }
}
