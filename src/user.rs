use bcrypt::{hash, verify, DEFAULT_COST};

pub enum LoginState {
    Connected,
    EnteringUsername,
    EnteringPassword,
    LoggedIn,
}

pub struct User {
    pub username: String,
    pub password_hash: String,
    pub login_state: LoginState,
}

impl User {
    pub fn new(username: &str, password: &str) -> Self {
        let password_hash = hash(password, DEFAULT_COST).unwrap();
        User {
            username: username.to_string(),
            password_hash,
            login_state: LoginState::Connected,
        }
    }

    pub fn verify_password(&self, password: &str) -> bool {
        verify(password, &self.password_hash).unwrap_or(false)
    }
}
