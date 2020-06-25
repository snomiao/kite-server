//! This is the freshman module, which is a part of sit-kite project.
//! Freshman module, as a tool, allows freshmen query their dormitory, counselor
//! and classmates.
//! In the design of this module, we use word "account" to express student id,
//! name or admission ticket number, when the word "secret" used as "password".
//! Usually, secret is the six right characters of their id card number.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgQueryAs, PgRow};

use crate::error::{Result, ServerError};
use futures::future::{ready, Ready};
use futures::StreamExt;

/// FreshmanEnv
/// Used to express campus, dormitory, counselor and other environment variables
/// for each new student.
/// Note: This structure is used to query only.
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct FreshmanBasic {
    pub uid: Option<i32>,
    /// student id.
    #[serde(rename(serialize = "studentId"))]
    pub student_id: String,
    /// Freshman college
    pub college: String,
    /// Freshman major
    pub major: String,
    /// campus of Fengxian or Xuhui.
    pub campus: String,
    /// like "1号楼". For Xuhui has some buildings named like "南1号楼", we use a string.
    pub building: String,
    /// like "101"
    pub room: i32,
    /// like "101-1"
    pub bed: String,
    /// Counselor's name
    pub counselor_name: String,
    /// Counselor's telephone
    pub counselor_tel: String,
    /// Allow people in the same city access one's contact details.
    pub private: bool,
}

/// This structure is of one student, which can be used in
/// show their classmates, roommates and people they may recognize.
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct NewMate {
    /// Freshman college
    pub college: String,
    /// Freshman major
    pub major: String,
    /// Freshman name
    pub name: String,
    /// Province, with out postfix "省"
    pub province: Option<String>,
    /// like "1号楼". For Xuhui has some buildings named like "南1号楼", we use a string.
    pub building: String,
    /// like "101"
    pub room: i32,
    /// Bed number, like "202-1"
    pub bed: String,
    /// last time the user access freshman system.
    #[serde(rename(serialize = "lastSeen"))]
    pub last_seen: Option<NaiveDateTime>,
    /// Avatar of the user
    // TODO: pub avatar: Option<String>,
    /// Contact detail like wechat, qq, telephone...
    pub contact: Option<serde_json::Value>,
}

/// Information about people you might know
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct PeopleFamiliar {
    /// Name of the people may recognize.
    pub name: String,
    /// College
    pub college: String,
    /// City where the people in
    pub city: String,
    /// Avatar
    pub avatar: String,
    /// Contact details.
    pub contact: String,
}

#[derive(Debug, Fail, ToPrimitive)]
pub enum FreshmanError {
    #[fail(display = "无匹配的新生数据")]
    NoSuchAccount = 18,
    #[fail(display = "账户不匹配")]
    DismatchAccount = 19,
    #[fail(display = "已绑定")]
    BoundAlready = 20,
    #[fail(display = "未登录")]
    Forbidden = 21,
    #[fail(display = "需要凭据")]
    SecretNeeded = 22,
}

pub async fn get_stduent_id_by_account(
    client: &PgPool,
    account: &String,
    secret: &String,
) -> Result<String> {
    // Select 学号、姓名、准考证号 and secret to select.
    let student_id: Option<(String,)> = sqlx::query_as(
        "SELECT student_id FROM students \
            WHERE (name = $1 or student_id = $1 or ticket = $1) and secret = $2 LIMIT 1",
    )
    .bind(account)
    .bind(secret)
    .fetch_optional(client)
    .await?;
    match student_id {
        Some(valid_id) => Ok(valid_id.0),
        None => Err(ServerError::new(FreshmanError::NoSuchAccount)),
    }
}

pub async fn is_account_bound(client: &PgPool, account: &String, secret: &String) -> Result<bool> {
    let row: Option<(u32,)> = sqlx::query_as(
        "SELECT 1 FROM students \
        WHERE (name = $1 or student_id = $1 or ticket = $1) and secret = $2 and uid is not null",
    )
    .bind(account)
    .bind(secret)
    .fetch_optional(client)
    .await?;
    Ok(row.is_some())
}

pub async fn is_uid_bound_with(client: &PgPool, uid: i32, account: &String) -> Result<bool> {
    let row: Option<(u32,)> = sqlx::query_as(
        "SELECT 1 FROM students \
        WHERE uid = $1 AND (name = $2 or student_id = $2 or ticket = $2)",
    )
    .bind(uid)
    .bind(account)
    .fetch_optional(client)
    .await?;
    Ok(row.is_some())
}

/// Bind account(name, student_id, ticket) to uid.
/// Note: There are two SQL queries in the function, and the first is the checking of whether
/// they are bound. so data synchronization problems may occur.
/// While if the account is not existing, it will return FreshmanError::NoSuchAccount.
/// Normally, it returns a String as student_id.
pub async fn bind_account(
    client: &PgPool,
    uid: i32,
    account: &String,
    secret: &String,
) -> Result<String> {
    let student_id: Option<(String,)> = sqlx::query_as(
        "UPDATE student SET uid = $1 \
        WHERE (name = $2 or student_id = $2 or ticket = $2) and secret = $3 and uid is null \
        RETURNING student_id",
    )
    .bind(uid)
    .bind(account)
    .bind(secret)
    .fetch_optional(client)
    .await?;

    // If the account is not existing, return FreshmanError::NoSuchAccount,
    // else return student_id.
    match student_id {
        Some(valid_id) => Ok(valid_id.0),
        None => Err(ServerError::new(FreshmanError::NoSuchAccount)),
    }
}

pub async fn get_basic_info(client: &PgPool, uid: i32) -> Result<FreshmanBasic> {
    let student_basic: Option<FreshmanBasic> = sqlx::query_as::<_, FreshmanBasic>(
        "SELECT \
            uid, student_id, college, major, campus, building, room, bed, \
            counselor_name, counselor_tel, private \
            FROM students \
            WHERE uid = $1",
    )
    .bind(uid)
    .fetch_optional(client)
    .await?;

    match student_basic {
        Some(e) => Ok(e),
        None => Err(ServerError::new(FreshmanError::NoSuchAccount)),
    }
}

pub async fn update_contact_by_uid(
    client: &PgPool,
    uid: i32,
    new_contact: &serde_json::Value,
) -> Result<()> {
    let affected_count = sqlx::query("UPDATE students SET contact = $1 WHERE uid = $2")
        .bind(new_contact)
        .bind(uid)
        .execute(client)
        .await?;

    Ok(())
}

pub async fn set_visibility(client: &PgPool, uid: i32, visible: bool) -> Result<()> {
    let affected_count = sqlx::query("UPDATE students SET visibility = $1 WHERE uid = $2")
        .bind(visible)
        .bind(uid)
        .execute(client)
        .await?;

    Ok(())
}

pub async fn get_classmates(client: &PgPool, uid: i32) -> Result<Vec<NewMate>> {
    let classmates: Vec<NewMate> = sqlx::query_as(
        "SELECT college, major, name, province, building, room, bed, last_seen \
            FROM freshman.students as t \
            WHERE class = (SELECT class FROM t WHERE uid = $1)",
    )
    .bind(uid)
    .fetch_all(client)
    .await?;

    Ok(classmates)
}

pub async fn get_roommates(client: &PgPool, uid: i32) -> Result<Vec<NewMate>> {
    let roommates: Vec<NewMate> = sqlx::query_as(
        "SELECT college, major, name, province, building, room, bed, last_seen \
            FROM freshman.students as t \
            WHERE class = (SELECT class FROM t WHERE uid = $1)",
    )
    .bind(uid)
    .fetch_all(client)
    .await?;

    Ok(roommates)
}

pub async fn get_people_familiar(client: &PgPool, uid: i32) -> Result<Vec<PeopleFamiliar>> {
    let people_familiar: Vec<PeopleFamiliar> = sqlx::query_as(
        "SELECT DISTINCT name, college, stu.city, avatar, contact
            FROM freshman.students AS stu
            LEFT JOIN public.persons AS person
            ON stu.uid = person.uid
            INNER JOIN (
                    SELECT graduated_from, city, postcode FROM freshman.students WHERE uid = '$1' LIMIT 1
                ) self
            ON
                ((stu.graduated_from = self.graduated_from)
                OR stu.city = self.city
                OR stu.postcode / 1000 = self.postcode / 1000)
                AND visible = true
                AND uid <> '$1';")
        .bind(uid)
        .fetch_all(client)
        .await?;

    Ok(people_familiar)
}
