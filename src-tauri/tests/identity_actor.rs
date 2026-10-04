use async_trait::async_trait;
use revue_office_lib::application::identity::{
    Actor, ActorKind, IdentityAdapter, IdentityError, IdentityProof,
};
use revue_office_lib::bootstrap::config::IdentityPolicy;
use revue_office_lib::infrastructure::identity::{IdentityUserLookup, JwtIdentityAdapter};
use secrecy::SecretString;
use std::sync::Arc;

struct Lookup(Option<Actor>);
#[async_trait]
impl IdentityUserLookup for Lookup {
    async fn find(&self, _: &str) -> Result<Option<Actor>, IdentityError> {
        Ok(self.0.clone())
    }
}
fn signing_material() -> String {
    "synthetic-test-signing-material-never-for-runtime".into()
}
fn proof(exp: i64) -> IdentityProof {
    let payload =
        serde_json::json!({"sub":"owner", "username":"untrusted", "role":"admin", "exp":exp});
    IdentityProof(SecretString::new(
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &payload,
            &jsonwebtoken::EncodingKey::from_secret(signing_material().as_bytes()),
        )
        .unwrap(),
    ))
}
fn adapter(actor: Option<Actor>, policy: IdentityPolicy) -> JwtIdentityAdapter {
    JwtIdentityAdapter::new(
        SecretString::new(signing_material()),
        policy,
        Arc::new(Lookup(actor)),
    )
}
#[tokio::test]
async fn jwt_uses_current_identity_not_claimed_roles() {
    let actor = adapter(Some(Actor::user("owner")), IdentityPolicy::LocalGuest)
        .authenticate(proof(chrono::Utc::now().timestamp() + 3600))
        .await
        .unwrap();
    assert!(actor.has_role("user"));
    assert!(!actor.has_role("admin"));
}
#[tokio::test]
async fn invalid_expired_and_deleted_identities_are_rejected() {
    let id = adapter(Some(Actor::user("owner")), IdentityPolicy::LocalGuest);
    assert!(
        id.authenticate(IdentityProof(SecretString::new("synthetic-invalid".into())))
            .await
            .is_err()
    );
    assert!(id.authenticate(proof(1)).await.is_err());
    assert!(
        adapter(None, IdentityPolicy::LocalGuest)
            .authenticate(proof(chrono::Utc::now().timestamp() + 3600))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn local_guest_remains_valid_but_server_rejects_guest() {
    let mut guest = Actor::user("guest-owner");
    guest.kind = ActorKind::Guest;
    let exp = chrono::Utc::now().timestamp() + 3600;
    assert!(
        adapter(Some(guest.clone()), IdentityPolicy::LocalGuest)
            .authenticate(proof(exp))
            .await
            .is_ok()
    );
    assert!(matches!(
        adapter(Some(guest), IdentityPolicy::JwtOnly)
            .authenticate(proof(exp))
            .await,
        Err(IdentityError::Forbidden)
    ));
}
struct FakeIdentity;
#[async_trait]
impl IdentityAdapter for FakeIdentity {
    async fn authenticate(&self, _: IdentityProof) -> Result<Actor, IdentityError> {
        Ok(Actor::user("fake-owner"))
    }
}

#[tokio::test]
async fn transport_extracts_actor_with_replaceable_identity_adapter() {
    use axum::extract::FromRequestParts;
    use revue_office_lib::transport::http::auth::{AuthenticatedActor, IdentityState};

    #[derive(Clone, axum::extract::FromRef)]
    struct TestHttpState {
        identity: IdentityState,
    }

    let state = TestHttpState {
        identity: IdentityState(Arc::new(FakeIdentity)),
    };
    let request = axum::http::Request::builder()
        .header("Authorization", "Bearer synthetic-proof")
        .body(())
        .unwrap();
    let (mut parts, _) = request.into_parts();
    let actor = AuthenticatedActor::from_request_parts(&mut parts, &state).await;
    assert!(actor.is_ok());
    assert!(actor.ok().unwrap().0.owns("fake-owner"));
    parts.headers.remove("Authorization");
    assert!(
        AuthenticatedActor::from_request_parts(&mut parts, &state)
            .await
            .is_err()
    );
    parts
        .headers
        .insert("Authorization", "Basic synthetic-proof".parse().unwrap());
    assert!(
        AuthenticatedActor::from_request_parts(&mut parts, &state)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sqlite_account_adapter_preserves_register_login_and_current_account_semantics() {
    use revue_office_lib::application::identity::IdentityApplicationService;
    use revue_office_lib::infrastructure::identity::AuthEndpoints;
    use revue_office_lib::infrastructure::persistence::sqlite::identity::SqliteAccountRepository;

    let accounts = Arc::new(
        SqliteAccountRepository::connect("sqlite::memory:", 1)
            .await
            .unwrap(),
    );
    let application = IdentityApplicationService::new(Arc::new(AuthEndpoints::new(
        accounts,
        IdentityPolicy::LocalGuest,
        SecretString::new(signing_material()),
        24,
    )));
    let registered = application
        .register(
            "typed-account",
            Some("typed@example.test"),
            SecretString::new("synthetic-password".into()),
        )
        .await
        .unwrap();
    let logged_in = application
        .login(
            "typed-account",
            SecretString::new("synthetic-password".into()),
        )
        .await
        .unwrap();
    assert_eq!(logged_in.account.id, registered.account.id);
    let current = application
        .current(&Actor::user(&registered.account.id))
        .await
        .unwrap();
    assert_eq!(current.username, "typed-account");
    assert_eq!(current.email.as_deref(), Some("typed@example.test"));
}

#[test]
fn application_permissions_need_only_actor_semantics() {
    let actor = Actor::user("owner");
    assert!(actor.owns("owner"));
    assert!(!actor.owns("other"));
    assert!(actor.has_role("user"));
    assert!(!actor.has_role("admin"));
}
