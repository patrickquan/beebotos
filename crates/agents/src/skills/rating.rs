//! Skill Rating System
//!
//! SQLite-backed persistent storage for skill ratings and reviews.

use sqlx::SqlitePool;

/// A single rating entry
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SkillRating {
    pub id: i64,
    pub skill_id: String,
    pub user_id: String,
    pub rating: i32,
    pub review: Option<String>,
    pub created_at: i64,
}

/// Rating summary for a skill
#[derive(Debug, Clone, Default)]
pub struct RatingSummary {
    pub average_rating: f64,
    pub total_ratings: u32,
}

/// Skill rating store backed by SQLite
#[derive(Clone)]
pub struct SkillRatingStore {
    db: SqlitePool,
}

impl SkillRatingStore {
    /// Create a new rating store
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    /// Rate a skill (insert or update)
    pub async fn rate(
        &self,
        skill_id: &str,
        user_id: &str,
        rating: u32,
        review: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let rating_i32 = rating as i32;
        let review_str = review.unwrap_or("");

        sqlx::query(
            r#"
            INSERT INTO skill_ratings (skill_id, user_id, rating, review, created_at)
            VALUES (?1, ?2, ?3, ?4, unixepoch())
            ON CONFLICT(skill_id, user_id) DO UPDATE SET
                rating = excluded.rating,
                review = excluded.review,
                created_at = excluded.created_at
            "#,
        )
        .bind(skill_id)
        .bind(user_id)
        .bind(rating_i32)
        .bind(review_str)
        .execute(&self.db)
        .await?;

        Ok(())
    }

    /// Get rating summary for a skill
    pub async fn get_summary(&self, skill_id: &str) -> Result<RatingSummary, sqlx::Error> {
        let row: (Option<f64>, i64) =
            sqlx::query_as("SELECT AVG(rating), COUNT(*) FROM skill_ratings WHERE skill_id = ?1")
                .bind(skill_id)
                .fetch_one(&self.db)
                .await?;

        Ok(RatingSummary {
            average_rating: row.0.unwrap_or(0.0),
            total_ratings: row.1 as u32,
        })
    }

    /// List ratings for a skill with pagination
    pub async fn list_ratings(
        &self,
		
		             <available_skills>\n\
             </available_skills>\n\
             \n\
             No skills are currently installed. If asked about available skills, report that none are installed."
        );
    }

    // 按使用频率排序，高优先级 skill 在前
    let mut skills = skills;
    skills.sort_by(|a, b| b.usage_count.cmp(&a.usage_count));
    let skills = &skills[..skills.len().min(MAX_SKILLS_IN_PROMPT)];

    // 尝试 full 模式
    let full = build_xml(skills, true);
    if full.chars().count() <= MAX_SKILL_PROMPT_CHARS {
        return full;
    }

    // fallback 1: compact 模式（去掉 description）
    let compact = build_xml(skills, false);
    if compact.chars().count() <= MAX_SKILL_PROMPT_CHARS {
        return compact;
    }

    // fallback 2: 截断到预算
    truncate_to_budget(compact, MAX_SKILL_PROMPT_CHARS)
}

fn build_xml(skills: &[crate::skills::registry::RegisteredSkill], with_desc: bool) -> String {
    let mut lines: Vec<String> = vec![
        String::new(),
        String::from(
            "The following skills provide specialized instructions for specific tasks.",
        ),
        String::from(
            "Use the read tool to load a skill's file when the task matches its description.",
        ),
        String::from(
            "When a skill file references a relative path, resolve it against the skill directory",
        ),
        String::from(
            "(parent of SKILL.md / dirname of the path) and use that absolute path in tool commands.",
        ),
        String::from(
            "When executing skill scripts with the exec tool, ALWAYS set the cwd parameter to the skill directory",
        ),
        String::from(
            "so that relative paths in commands work correctly.",
        ),
        String::new(),
        skill_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SkillRating>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT id, skill_id, user_id, rating, review, created_at
            FROM skill_ratings
            WHERE skill_id = ?1
            ORDER BY created_at DESC
            LIMIT ?2 OFFSET ?3
            "#,
        )
        .bind(skill_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            r#"
            CREATE TABLE skill_ratings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                skill_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                rating INTEGER NOT NULL CHECK(rating >= 1 AND rating <= 5),
                review TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                UNIQUE(skill_id, user_id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn test_rate_and_summary() {
        let db = create_test_db().await;
        let store = SkillRatingStore::new(db);

        store
            .rate("skill-1", "user-a", 5, Some("Great!"))
            .await
            .unwrap();
        store.rate("skill-1", "user-b", 3, None).await.unwrap();
        store.rate("skill-1", "user-c", 4, None).await.unwrap();

        let summary = store.get_summary("skill-1").await.unwrap();
        assert_eq!(summary.total_ratings, 3);
        assert!((summary.average_rating - 4.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_update_rating() {
        let db = create_test_db().await;
        let store = SkillRatingStore::new(db);

        store.rate("skill-1", "user-a", 2, None).await.unwrap();
        store.rate("skill-1", "user-a", 5, None).await.unwrap();

        let summary = store.get_summary("skill-1").await.unwrap();
        assert_eq!(summary.total_ratings, 1);
        assert!((summary.average_rating - 5.0).abs() < 0.01);
    }
}
