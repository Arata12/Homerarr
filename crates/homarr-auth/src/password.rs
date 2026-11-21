use homarr_common::{HomarrError, HomarrResult};

const BCRYPT_COST: u32 = 10;

pub async fn hash_password(password: &str) -> HomarrResult<String> {
    let pw = password.to_string();
    tokio::task::spawn_blocking(move || bcrypt::hash(pw, BCRYPT_COST))
        .await
        .map_err(|err| HomarrError::Internal(format!("Password hashing task panicked: {err}")))?
        .map_err(|err| HomarrError::Internal(format!("Failed to hash password: {err}")))
}

pub async fn verify_password(password: &str, hash: &str) -> HomarrResult<bool> {
    let pw = password.to_string();
    let h = hash.to_string();
    tokio::task::spawn_blocking(move || bcrypt::verify(pw, &h))
        .await
        .map_err(|err| HomarrError::Internal(format!("Password verify task panicked: {err}")))?
        .map_err(|err| HomarrError::Internal(format!("Failed to verify password: {err}")))
}

/// Well-formed bcrypt hash (same cost as `BCRYPT_COST`) that matches no password. Verifying against
/// it keeps login timing the same for unknown users as for known ones.
const DUMMY_HASH: &str = "$2b$10$abcdefghijklmnopqrstuuABCDEFGHIJKLMNOPQRSTUVWXYZ0123O";

pub async fn verify_dummy_password(password: &str) {
    let _ = verify_password(password, DUMMY_HASH).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dummy_hash_is_valid_and_matches_cost() {
        assert!(DUMMY_HASH.starts_with(&format!("$2b${BCRYPT_COST}$")));
        assert!(!verify_password("anything", DUMMY_HASH).await.unwrap());
    }
}
