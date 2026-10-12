//! The repository contract: one set of checks that every `TrackRepository` must pass
//! (database.md AC-4). The fake runs it in a unit test, and `PgTrackRepository` runs it
//! against a real database.
//!
//! Test-support code: a failed check panics by design, so the no-panic lints are off here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use domain::track::{NewTrack, Track, TrackArtist, TrackDuration, TrackTitle};
use uuid::Uuid;

use super::ports::{InsertOutcome, TrackRepository};
use crate::pagination::PageRequest;

fn track(title: &str, ms: i32) -> NewTrack {
    NewTrack::new(
        Uuid::now_v7(),
        TrackTitle::try_from(title.to_owned()).unwrap(),
        TrackArtist::try_from("Contract Artist".to_owned()).unwrap(),
        TrackDuration::try_from(ms).unwrap(),
    )
}

/// Runs every check against `repo`, which must be empty.
pub async fn track_repository_contract(repo: &dyn TrackRepository) {
    insert_and_find(repo).await;
    insert_twice_keeps_the_first(repo).await;
    update_and_delete(repo).await;
    keyset_paging(repo).await;
}

async fn insert_and_find(repo: &dyn TrackRepository) {
    let new = track("Insert", 1000);
    let InsertOutcome::Inserted(stored) = repo.insert(&new).await.unwrap() else {
        panic!("a new id must be Inserted");
    };
    assert_eq!(stored.id(), new.id());
    assert_eq!(stored.title(), new.title());
    assert_eq!(stored.artist(), new.artist());
    assert_eq!(stored.duration(), new.duration());
    assert_eq!(stored.version(), 1);
    assert_eq!(stored.created_at(), stored.updated_at());

    assert_eq!(repo.find_by_id(new.id()).await.unwrap(), Some(stored));
    assert_eq!(repo.find_by_id(Uuid::now_v7()).await.unwrap(), None);
    assert!(repo.delete(new.id()).await.unwrap());
}

async fn insert_twice_keeps_the_first(repo: &dyn TrackRepository) {
    let first = track("First", 1000);
    repo.insert(&first).await.unwrap();
    let second = NewTrack::new(
        first.id(),
        TrackTitle::try_from("Second".to_owned()).unwrap(),
        first.artist().clone(),
        first.duration(),
    );
    let InsertOutcome::AlreadyExists(existing) = repo.insert(&second).await.unwrap() else {
        panic!("a known id must be AlreadyExists");
    };
    assert_eq!(existing.title().as_str(), "First");
    let found = repo.find_by_id(first.id()).await.unwrap().unwrap();
    assert_eq!(
        found.title().as_str(),
        "First",
        "the stored row is unchanged"
    );
    assert!(repo.delete(first.id()).await.unwrap());
}

async fn update_and_delete(repo: &dyn TrackRepository) {
    let original = track("Before", 1000);
    let InsertOutcome::Inserted(before) = repo.insert(&original).await.unwrap() else {
        panic!("a new id must be Inserted");
    };

    let mut edited = before.clone();
    edited.replace(
        TrackTitle::try_from("After".to_owned()).unwrap(),
        TrackArtist::try_from("Other Artist".to_owned()).unwrap(),
        TrackDuration::try_from(2000).unwrap(),
    );
    let updated = repo.update(&edited).await.unwrap();
    assert_eq!(updated.title().as_str(), "After");
    assert_eq!(updated.artist().as_str(), "Other Artist");
    assert_eq!(updated.duration().get(), 2000);
    assert_eq!(updated.version(), before.version() + 1);
    assert!(updated.updated_at() >= before.updated_at());
    assert_eq!(updated.id(), before.id());
    assert_eq!(updated.created_at(), before.created_at());
    assert_eq!(repo.find_by_id(before.id()).await.unwrap(), Some(updated));

    assert!(repo.delete(before.id()).await.unwrap());
    // Updating a track that is gone is NotFound.
    assert!(matches!(
        repo.update(&edited).await,
        Err(crate::error::AppError::NotFound)
    ));
    assert!(!repo.delete(before.id()).await.unwrap(), "second delete");
    assert_eq!(repo.find_by_id(before.id()).await.unwrap(), None);
}

async fn keyset_paging(repo: &dyn TrackRepository) {
    // Ids from `now_v7` rise with each call, so insertion order is oldest first.
    let mut ids = Vec::new();
    for n in 0..5 {
        let t = track(&format!("Page {n}"), 1000 + n);
        ids.push(t.id());
        repo.insert(&t).await.unwrap();
    }
    ids.sort_unstable_by(|a, b| b.cmp(a));

    let request = |after| PageRequest::new(Some(2), after).unwrap();
    let first = repo.list(request(None)).await.unwrap();
    assert_eq!(ids_of(&first.items), ids[..2]);
    assert_eq!(first.next, Some(ids[1]));
    let second = repo.list(request(first.next)).await.unwrap();
    assert_eq!(ids_of(&second.items), ids[2..4]);
    assert_eq!(second.next, Some(ids[3]));
    let third = repo.list(request(second.next)).await.unwrap();
    assert_eq!(ids_of(&third.items), ids[4..]);
    assert_eq!(third.next, None);

    // A page that is exactly full has no next page.
    let exact = repo
        .list(PageRequest::new(Some(5), None).unwrap())
        .await
        .unwrap();
    assert_eq!(ids_of(&exact.items), ids);
    assert_eq!(exact.next, None);

    for id in ids {
        assert!(repo.delete(id).await.unwrap());
    }
    let empty = repo.list(request(None)).await.unwrap();
    assert!(empty.items.is_empty() && empty.next.is_none());
}

fn ids_of(tracks: &[Track]) -> Vec<Uuid> {
    tracks.iter().map(Track::id).collect()
}
