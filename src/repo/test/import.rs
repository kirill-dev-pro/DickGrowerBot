use crate::repo;
use crate::repo::test::dicks::{check_dick, create_dick, create_user};
use crate::repo::test::{start_postgres, CHAT_ID, UID};
use crate::repo::ExternalUser;
use teloxide::types::{ChatId, UserId};

#[tokio::test]
async fn test_all() {
    let (_container, db) = start_postgres().await;
    let import = repo::Import::new(db.clone());
    let chat_id = ChatId(CHAT_ID);

    create_user(&db).await;
    create_dick(&db).await;

    let u = import
        .get_imported_users(chat_id)
        .await
        .expect("couldn't fetch the empty list");
    assert_eq!(u.len(), 0);

    let length = 5;
    let users = vec![ExternalUser::new(UserId(UID as u64), length)];
    import
        .import(chat_id, &users)
        .await
        .expect("couldn't import users");

    let u = import
        .get_imported_users(chat_id)
        .await
        .expect("couldn't fetch the list of imported users");
    assert_eq!(u.len(), 1);
    assert_eq!(u, users);

    check_dick(&db, length).await;
}
