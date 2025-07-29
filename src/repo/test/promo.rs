use crate::repo;
use crate::repo::test::dicks::{check_dick, create_dick, create_user};
use crate::repo::test::{start_postgres, UID};
use crate::repo::PromoCodeParams;
use teloxide::types::UserId;

const PROMO_CODE: &str = "test10";
const PROMO_CODE_UPPERCASE: &str = "TEST10";
const PROMO_BONUS: u32 = 10;

#[tokio::test]
async fn activate() {
    let (_container, db) = start_postgres().await;

    let promo = repo::Promo::new(db.clone());
    promo
        .create(PromoCodeParams {
            code: PROMO_CODE.to_owned(),
            bonus_length: PROMO_BONUS,
            capacity: 1,
        })
        .await
        .expect("couldn't create a promo code");

    create_user(&db).await;
    create_dick(&db).await;
    let res = promo
        .activate(UserId(UID as u64), PROMO_CODE_UPPERCASE)
        .await
        .expect("couldn't activate the promo code");
    assert_eq!(res.chats_affected, 1);
    assert_eq!(res.bonus_length, PROMO_BONUS as i32);

    check_dick(&db, PROMO_BONUS).await;

    let res = promo.activate(UserId(UID as u64), PROMO_CODE).await;
    assert!(res.is_err());
}
