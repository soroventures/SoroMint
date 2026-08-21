//! Role-based access control.
//!
//! The root `Admin` implicitly holds every role, so a fresh deployment is
//! usable before any delegation happens. Delegated roles are additive and
//! narrow:
//!
//! * `PolicyAdmin` moves the safety goalposts (thresholds, blocklist root).
//! * `CircuitAdmin` moves the *proof system* (verifying keys).
//! * `Pauser` can stop the world but not restart it.
//! * `Consumer` is held by lifecycle contracts, not humans.
//!
//! Splitting policy from circuits matters: a compromised policy key can only
//! loosen thresholds within the constraints the circuit already enforces, while
//! a compromised circuit key could install a verifying key that accepts
//! anything. The circuit path is therefore additionally protected by a
//! timelock (see [`crate::registry`]).

use soroban_sdk::{Address, Env};

use crate::errors::Error;
use crate::storage;
use crate::types::Role;

/// Assert `caller` authorized this invocation and holds `role` (or is Admin).
pub fn require_role(env: &Env, caller: &Address, role: Role) -> Result<(), Error> {
    caller.require_auth();
    if is_admin(env, caller)? || storage::has_role(env, role, caller) {
        Ok(())
    } else {
        Err(Error::Unauthorized)
    }
}

/// Assert `caller` authorized this invocation and is the root admin.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), Error> {
    caller.require_auth();
    if is_admin(env, caller)? {
        Ok(())
    } else {
        Err(Error::Unauthorized)
    }
}

/// Role check without an auth requirement, for read-only queries.
pub fn check_role(env: &Env, who: &Address, role: Role) -> bool {
    match is_admin(env, who) {
        Ok(true) => true,
        _ => storage::has_role(env, role, who),
    }
}

pub fn is_admin(env: &Env, who: &Address) -> Result<bool, Error> {
    let admin = storage::get_admin(env)?;
    Ok(&admin == who)
}

/// Grant `role` to `who`. Admin-only.
pub fn grant(env: &Env, caller: &Address, role: Role, who: &Address) -> Result<(), Error> {
    require_admin(env, caller)?;
    if role == Role::Admin {
        // The root admin is a single address managed through the two-step
        // transfer flow, not a set. Granting it as a plain role would create a
        // second admin with no transfer ceremony behind it.
        return Err(Error::InvalidRoleTarget);
    }
    storage::grant_role(env, role, who);
    crate::events::role_granted(env, role, who, caller);
    Ok(())
}

/// Revoke `role` from `who`. Admin-only.
pub fn revoke(env: &Env, caller: &Address, role: Role, who: &Address) -> Result<(), Error> {
    require_admin(env, caller)?;
    if role == Role::Admin {
        return Err(Error::InvalidRoleTarget);
    }
    storage::revoke_role(env, role, who);
    crate::events::role_revoked(env, role, who, caller);
    Ok(())
}

/// Step one of the admin handoff: the incumbent nominates a successor.
///
/// A direct `set_admin` is a foot-gun — one typo permanently bricks governance.
/// The nominee must call [`accept_admin`] from their own key, which proves the
/// address is both correct and controlled.
pub fn transfer_admin(env: &Env, caller: &Address, new_admin: &Address) -> Result<(), Error> {
    require_admin(env, caller)?;
    if new_admin == caller {
        return Err(Error::InvalidRoleTarget);
    }
    storage::set_pending_admin(env, new_admin);
    crate::events::admin_transfer_started(env, caller, new_admin);
    Ok(())
}

/// Step two: the nominee claims the role.
pub fn accept_admin(env: &Env, caller: &Address) -> Result<(), Error> {
    caller.require_auth();
    let pending = storage::get_pending_admin(env).ok_or(Error::NoPendingTransfer)?;
    if &pending != caller {
        return Err(Error::NoPendingTransfer);
    }
    let previous = storage::get_admin(env)?;
    storage::set_admin(env, caller);
    storage::clear_pending_admin(env);
    crate::events::admin_transfer_completed(env, &previous, caller);
    Ok(())
}

/// Abort a pending handoff.
pub fn cancel_admin_transfer(env: &Env, caller: &Address) -> Result<(), Error> {
    require_admin(env, caller)?;
    if storage::get_pending_admin(env).is_none() {
        return Err(Error::NoPendingTransfer);
    }
    storage::clear_pending_admin(env);
    Ok(())
}
