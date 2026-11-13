//! Team scans in the dashboard's History.
//!
//! - The owner can pick "My scans", "All team scans", or one member.
//! - A member can pick "My scans" or one teammate (the owner too), one
//!   person at a time. Members do not get "All team scans".
//! - A scan belongs to the team it was made in (analysis.team_owner_id,
//!   set when it is saved; migration 0017). So a member's scans from
//!   before they joined are not shared, and their team scans stay with
//!   the team after they leave. An owner's older scans (made before the
//!   column existed) count as the team's.
//! - Only people on a paid plan with a team see any of this.

use crate::models::history::HistoryItem;
use crate::services::features::team::plan_holder;
use serde::Serialize;
use sqlx::{Error, FromRow, Pool, Postgres, query_as, query_scalar};
use uuid::Uuid;

/// One person the viewer can pick in the History dropdown.
#[derive(Debug, Serialize, FromRow)]
pub struct TeamPerson {
    pub id: Uuid,
    pub email: String,
    pub is_owner: bool,
}

/// One History row of a team list, with who scanned it.
#[derive(Debug, FromRow)]
pub struct TeamHistoryItem {
    #[sqlx(flatten)]
    pub item: HistoryItem,
    pub scanned_by_id: Uuid,
    pub scanned_by: String,
}

/// Which History list the viewer asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryWho {
    Mine,
    AllTeam,
    Person(Uuid),
}

impl HistoryWho {
    /// "team" = all team scans, a user ID = that person, anything else
    /// (or nothing) = my scans.
    pub fn parse(text: Option<&str>) -> HistoryWho {
        match text.map(str::trim) {
            Some("team") => HistoryWho::AllTeam,
            Some(t) => Uuid::parse_str(t).map_or(HistoryWho::Mine, HistoryWho::Person),
            None => HistoryWho::Mine,
        }
    }
}

/// The team whose scans this person can see (the owner's account ID),
/// or None when they are not in a paid team.
///
/// - A member with a seat on an owner's paid plan: that owner.
/// - An owner on a paid plan who has invited at least one person:
///   themselves.
pub async fn team_of(pool: &Pool<Postgres>, user_id: Uuid) -> Option<Uuid> {
    let holder = plan_holder(pool, user_id).await;
    if holder != user_id {
        return Some(holder);
    }
    let owns_team: bool = query_scalar(
        "SELECT EXISTS (SELECT 1 FROM subscriptions WHERE user_id = $1 AND status = 'active')
            AND EXISTS (SELECT 1 FROM team_members WHERE owner_id = $1)",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    owns_team.then_some(user_id)
}

/// The people in a team who have a Safely account: the owner first,
/// then the members in the order they were invited.
pub async fn team_people(pool: &Pool<Postgres>, owner_id: Uuid) -> Result<Vec<TeamPerson>, Error> {
    query_as::<_, TeamPerson>(
        "SELECT id, email, is_owner FROM (
            SELECT u.id, lower(u.email) AS email, TRUE AS is_owner, NULL::timestamptz AS since
            FROM users u WHERE u.id = $1
            UNION ALL
            SELECT u.id, m.email, FALSE, m.created_at
            FROM team_members m
            JOIN users u ON lower(u.email) = m.email
            WHERE m.owner_id = $1 AND u.id <> $1
         ) people
         ORDER BY is_owner DESC, since",
    )
    .bind(owner_id)
    .fetch_all(pool)
    .await
}

/// What the History dropdown offers this person: whether they are in a
/// team, whether they can see all team scans (the owner), and the
/// other people in the team.
pub async fn history_choices(
    pool: &Pool<Postgres>,
    user_id: Uuid,
) -> Result<(bool, bool, Vec<TeamPerson>), Error> {
    let Some(owner_id) = team_of(pool, user_id).await else {
        return Ok((false, false, Vec::new()));
    };
    let people = team_people(pool, owner_id)
        .await?
        .into_iter()
        .filter(|p| p.id != user_id)
        .collect();
    Ok((true, owner_id == user_id, people))
}

/// The team History list for the viewer: all team scans (owner only) or
/// one person's team scans. None when the viewer may not see it.
pub async fn team_history(
    pool: &Pool<Postgres>,
    viewer: Uuid,
    who: HistoryWho,
) -> Result<Option<Vec<TeamHistoryItem>>, Error> {
    let Some(owner_id) = team_of(pool, viewer).await else {
        return Ok(None);
    };
    let person = match who {
        HistoryWho::Mine => return Ok(None),
        HistoryWho::AllTeam if owner_id != viewer => return Ok(None),
        HistoryWho::AllTeam => None,
        HistoryWho::Person(id) => {
            let in_team = team_people(pool, owner_id)
                .await?
                .iter()
                .any(|p| p.id == id);
            if !in_team {
                return Ok(None);
            }
            Some(id)
        }
    };
    // "In team $1": made in the team, or an owner's older scan.
    let rows = query_as::<_, TeamHistoryItem>(
        "
        SELECT * FROM (
            SELECT DISTINCT ON (s.id, COALESCE(l.listing_id, l.listing_url))
                a.id,
                a.created_at,
                a.risk_score,
                a.risk_level,
                l.platform,
                l.title AS listing_title,
                l.listing_url,
                s.name AS seller_name,
                s.id AS seller_id,
                EXISTS (
                    SELECT 1 FROM fraud_reports fr
                    WHERE fr.seller_id = s.id
                      AND fr.user_id = a.user_id
                      AND fr.listing_url = l.listing_url
                ) AS reported,
                a.user_id AS scanned_by_id,
                lower(u.email) AS scanned_by
            FROM analysis a
            JOIN listings l ON a.listing_id = l.id
            JOIN sellers s ON l.seller_id = s.id
            JOIN users u ON u.id = a.user_id
            WHERE (a.team_owner_id = $1 OR (a.team_owner_id IS NULL AND a.user_id = $1))
              AND ($2::uuid IS NULL OR a.user_id = $2)
            ORDER BY s.id, COALESCE(l.listing_id, l.listing_url), a.created_at DESC
        ) sub
        ORDER BY created_at DESC
        LIMIT 200
        ",
    )
    .bind(owner_id)
    .bind(person)
    .fetch_all(pool)
    .await?;
    Ok(Some(rows))
}

/// Who made a scan, when the viewer may open it: the viewer themselves,
/// or a teammate when the scan belongs to the viewer's team. None when
/// the scan does not exist or is not theirs to see.
pub async fn scan_owner_for_viewer(
    pool: &Pool<Postgres>,
    analysis_id: Uuid,
    viewer: Uuid,
) -> Result<Option<Uuid>, Error> {
    let row: Option<(Option<Uuid>, Option<Uuid>)> =
        query_as("SELECT user_id, team_owner_id FROM analysis WHERE id = $1")
            .bind(analysis_id)
            .fetch_optional(pool)
            .await?;
    let Some((Some(scanner), team)) = row else {
        return Ok(None);
    };
    if scanner == viewer {
        return Ok(Some(viewer));
    }
    let Some(owner_id) = team_of(pool, viewer).await else {
        return Ok(None);
    };
    let in_team = team == Some(owner_id) || (team.is_none() && scanner == owner_id);
    Ok(in_team.then_some(scanner))
}

/// A teammate's email, for "Scanned by" on a result page.
pub async fn email_of(pool: &Pool<Postgres>, user_id: Uuid) -> Option<String> {
    query_scalar("SELECT lower(email) FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_history_choice() {
        assert_eq!(HistoryWho::parse(None), HistoryWho::Mine);
        assert_eq!(HistoryWho::parse(Some("me")), HistoryWho::Mine);
        assert_eq!(HistoryWho::parse(Some("team")), HistoryWho::AllTeam);
        let id = Uuid::now_v7();
        assert_eq!(
            HistoryWho::parse(Some(&id.to_string())),
            HistoryWho::Person(id)
        );
        assert_eq!(HistoryWho::parse(Some("nonsense")), HistoryWho::Mine);
    }
}
