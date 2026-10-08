//! Comptes de trading : les clés d'API des plateformes où l'utilisateur placera de vrais euros.
//!
//! Les clés ne vont JAMAIS dans la base, dans un fichier ni dans l'interface : elles sont
//! rangées dans le Gestionnaire d'identification de Windows (chiffré par Windows pour la
//! session de l'utilisateur), une entrée `PaperTrading/<plateforme>` par plateforme.
//! L'interface ne reçoit en retour que l'état (connecté ou non), les 4 derniers caractères
//! de la clé publique et la date d'enregistrement.
//!
//! Aujourd'hui l'application n'envoie aucun ordre : ces clés sont préparées pour la
//! connexion au trading réel, qui sera le seul changement le jour venu.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Taille maximale d'une entrée du Gestionnaire d'identification (CRED_MAX_CREDENTIAL_BLOB_SIZE).
const MAX_BLOB: usize = 5 * 512;
/// Un champ plus long n'est pas une clé d'API : c'est une erreur de collage.
const MAX_FIELD: usize = 2000;

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    pub id: &'static str,
    pub label: &'static str,
    /// Champ masqué à la saisie (secret, clé privée, phrase de passe).
    pub secret: bool,
    /// Saisie sur plusieurs lignes (clé privée au format PEM).
    pub multiline: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Platform {
    pub id: &'static str,
    pub name: &'static str,
    /// Page où créer une clé d'API sur la plateforme.
    pub keys_url: &'static str,
    pub fields: Vec<Field>,
}

const fn f(id: &'static str, label: &'static str, secret: bool) -> Field {
    Field { id, label, secret, multiline: false }
}

/// Plateformes au comptant en euros, avec une API de trading, ouvertes aux résidents de l'UE.
/// Le premier champ est toujours l'identifiant public de la clé (celui dont on montre la fin).
pub fn platforms() -> Vec<Platform> {
    vec![
        Platform { id: "bitvavo", name: "Bitvavo", keys_url: "https://account.bitvavo.com/user/api", fields: vec![f("key", "Clé d'API", false), f("secret", "Secret", true)] },
        Platform { id: "kraken", name: "Kraken", keys_url: "https://www.kraken.com/u/security/api", fields: vec![f("key", "Clé d'API", false), f("secret", "Clé privée", true)] },
        Platform {
            id: "coinbase",
            name: "Coinbase (Advanced Trade)",
            keys_url: "https://portal.cdp.coinbase.com/access/api",
            fields: vec![f("key", "Nom de la clé (organizations/…/apiKeys/…)", false), Field { id: "secret", label: "Clé privée (PEM)", secret: true, multiline: true }],
        },
        Platform { id: "bitstamp", name: "Bitstamp", keys_url: "https://www.bitstamp.net/account/security/api/", fields: vec![f("key", "Clé d'API", false), f("secret", "Secret", true)] },
        Platform { id: "okx", name: "OKX", keys_url: "https://www.okx.com/account/my-api", fields: vec![f("key", "Clé d'API", false), f("secret", "Clé secrète", true), f("passphrase", "Phrase de passe", true)] },
        Platform { id: "bybit", name: "Bybit EU", keys_url: "https://www.bybit.eu/app/user/api-management", fields: vec![f("key", "Clé d'API", false), f("secret", "Secret", true)] },
        Platform { id: "cryptocom", name: "Crypto.com Exchange", keys_url: "https://crypto.com/exchange/user/settings/api-management", fields: vec![f("key", "Clé d'API", false), f("secret", "Clé secrète", true)] },
        Platform { id: "onetrading", name: "One Trading (ex-Bitpanda Pro)", keys_url: "https://onetrading.com/", fields: vec![f("key", "Clé d'API", true)] },
    ]
}

pub fn find(id: &str) -> Option<Platform> {
    platforms().into_iter().find(|p| p.id == id)
}

/// Ce qui est rangé dans l'entrée de Windows : les champs saisis et la date d'enregistrement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stored {
    pub fields: BTreeMap<String, String>,
    pub saved_at: i64,
}

/// Ce que voit l'interface : jamais un secret.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AccountView {
    pub platform: String,
    pub connected: bool,
    /// Les 4 derniers caractères de l'identifiant public de la clé, précédés de « … ».
    pub key_hint: Option<String>,
    pub saved_at: Option<i64>,
}

/// Contrôle la saisie : plateforme connue, chaque champ présent, non vide, sans espace autour,
/// d'une longueur plausible, aucun champ inconnu. Rend les champs nettoyés.
pub fn validate(platform: &str, raw: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>, String> {
    let p = find(platform).ok_or_else(|| format!("plateforme inconnue : {platform}"))?;
    if let Some(extra) = raw.keys().find(|k| !p.fields.iter().any(|f| f.id == k.as_str())) {
        return Err(format!("champ inconnu pour {} : {extra}", p.name));
    }
    let mut out = BTreeMap::new();
    for fd in &p.fields {
        let v = raw.get(fd.id).map(|s| s.trim()).unwrap_or("");
        if v.is_empty() {
            return Err(format!("{} : « {} » est vide", p.name, fd.label));
        }
        if v.chars().count() > MAX_FIELD {
            return Err(format!("{} : « {} » est trop long ({} caractères au plus)", p.name, fd.label, MAX_FIELD));
        }
        if !fd.multiline && v.chars().any(char::is_whitespace) {
            return Err(format!("{} : « {} » contient un espace ou un retour à la ligne", p.name, fd.label));
        }
        out.insert(fd.id.to_string(), v.to_string());
    }
    Ok(out)
}

/// `…abcd` : assez pour reconnaître une clé, jamais assez pour la reconstituer.
pub fn hint(public_id: &str) -> String {
    let chars: Vec<char> = public_id.chars().collect();
    let tail: String = chars[chars.len().saturating_sub(4)..].iter().collect();
    format!("…{tail}")
}

/// Rangement des clés. Une implémentation en mémoire sert aux tests.
pub trait Vault {
    fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, target: &str, blob: &[u8]) -> Result<(), String>;
    fn delete(&self, target: &str) -> Result<(), String>;
}

pub struct Accounts<V: Vault> {
    vault: V,
    prefix: String,
}

impl<V: Vault> Accounts<V> {
    pub fn new(vault: V, prefix: &str) -> Self {
        Self { vault, prefix: prefix.to_string() }
    }

    fn target(&self, platform: &str) -> String {
        format!("{}/{platform}", self.prefix)
    }

    pub fn save(&self, platform: &str, raw: &BTreeMap<String, String>, now_ms: i64) -> Result<AccountView, String> {
        let fields = validate(platform, raw)?;
        let blob = serde_json::to_vec(&Stored { fields, saved_at: now_ms }).map_err(|e| e.to_string())?;
        if blob.len() > MAX_BLOB {
            return Err(format!("clés trop longues pour le Gestionnaire d'identification de Windows ({} octets au plus)", MAX_BLOB));
        }
        self.vault.write(&self.target(platform), &blob)?;
        self.view(platform)
    }

    pub fn view(&self, platform: &str) -> Result<AccountView, String> {
        let p = find(platform).ok_or_else(|| format!("plateforme inconnue : {platform}"))?;
        let none = AccountView { platform: platform.to_string(), connected: false, key_hint: None, saved_at: None };
        let Some(blob) = self.vault.read(&self.target(platform))? else { return Ok(none) };
        // Une entrée illisible (modifiée à la main) est signalée, pas prise pour une connexion.
        let s: Stored = serde_json::from_slice(&blob).map_err(|_| format!("{} : l'entrée enregistrée est illisible, supprime-la puis ressaisis la clé", p.name))?;
        let public = s.fields.get(p.fields[0].id).map(|k| hint(k));
        Ok(AccountView { platform: platform.to_string(), connected: true, key_hint: if p.fields[0].secret { None } else { public }, saved_at: Some(s.saved_at) })
    }

    pub fn list(&self) -> Result<Vec<AccountView>, String> {
        platforms().iter().map(|p| self.view(p.id)).collect()
    }

    pub fn delete(&self, platform: &str) -> Result<(), String> {
        find(platform).ok_or_else(|| format!("plateforme inconnue : {platform}"))?;
        self.vault.delete(&self.target(platform))
    }

    /// Pour la future connexion au trading réel : les champs en clair, jamais envoyés à l'interface.
    #[allow(dead_code)]
    pub fn credentials(&self, platform: &str) -> Result<Option<BTreeMap<String, String>>, String> {
        let Some(blob) = self.vault.read(&self.target(platform))? else { return Ok(None) };
        let s: Stored = serde_json::from_slice(&blob).map_err(|e| e.to_string())?;
        Ok(Some(s.fields))
    }
}

/// Gestionnaire d'identification de Windows (entrées génériques, persistance « machine locale »,
/// chiffrées par Windows pour l'utilisateur courant).
#[cfg(windows)]
pub struct WindowsVault;

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND, FILETIME};
    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    impl super::Vault for super::WindowsVault {
        fn read(&self, target: &str) -> Result<Option<Vec<u8>>, String> {
            let t = wide(target);
            let mut p: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: `t` est terminée par 0 ; `p` est libéré par CredFree après copie.
            unsafe {
                if CredReadW(t.as_ptr(), CRED_TYPE_GENERIC, 0, &mut p) == 0 {
                    let e = GetLastError();
                    return if e == ERROR_NOT_FOUND { Ok(None) } else { Err(format!("lecture de {target} impossible (erreur Windows {e})")) };
                }
                let c = &*p;
                let blob = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
                CredFree(p as *const _);
                Ok(Some(blob))
            }
        }

        fn write(&self, target: &str, blob: &[u8]) -> Result<(), String> {
            let mut t = wide(target);
            let mut user = wide("PaperTrading");
            let cred = CREDENTIALW {
                Flags: 0,
                Type: CRED_TYPE_GENERIC,
                TargetName: t.as_mut_ptr(),
                Comment: std::ptr::null_mut(),
                LastWritten: FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 },
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_ptr() as *mut u8,
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                AttributeCount: 0,
                Attributes: std::ptr::null_mut(),
                TargetAlias: std::ptr::null_mut(),
                UserName: user.as_mut_ptr(),
            };
            // SAFETY: tous les pointeurs vivent jusqu'à la fin de l'appel ; Windows copie le contenu.
            if unsafe { CredWriteW(&cred, 0) } == 0 {
                return Err(format!("enregistrement de {target} impossible (erreur Windows {})", unsafe { GetLastError() }));
            }
            Ok(())
        }

        fn delete(&self, target: &str) -> Result<(), String> {
            let t = wide(target);
            // SAFETY: `t` est terminée par 0.
            unsafe {
                if CredDeleteW(t.as_ptr(), CRED_TYPE_GENERIC, 0) == 0 {
                    let e = GetLastError();
                    if e != ERROR_NOT_FOUND {
                        return Err(format!("suppression de {target} impossible (erreur Windows {e})"));
                    }
                }
            }
            Ok(())
        }
    }
}

/// Entrées réelles : `PaperTrading/<plateforme>` dans le Gestionnaire d'identification.
fn real() -> Accounts<WindowsVault> {
    Accounts::new(WindowsVault, "PaperTrading")
}

#[derive(Serialize)]
pub struct AccountsPage {
    pub platforms: Vec<Platform>,
    pub accounts: Vec<AccountView>,
}

#[tauri::command]
pub fn list_accounts() -> Result<AccountsPage, String> {
    Ok(AccountsPage { platforms: platforms(), accounts: real().list()? })
}

#[tauri::command]
pub fn save_account(platform: String, fields: BTreeMap<String, String>) -> Result<AccountView, String> {
    real().save(&platform, &fields, pt_data::now_ms())
}

#[tauri::command]
pub fn delete_account(platform: String) -> Result<(), String> {
    real().delete(&platform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Mem(RefCell<BTreeMap<String, Vec<u8>>>);
    impl Vault for Mem {
        fn read(&self, t: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.0.borrow().get(t).cloned())
        }
        fn write(&self, t: &str, b: &[u8]) -> Result<(), String> {
            self.0.borrow_mut().insert(t.into(), b.into());
            Ok(())
        }
        fn delete(&self, t: &str) -> Result<(), String> {
            self.0.borrow_mut().remove(t);
            Ok(())
        }
    }

    fn m(kv: &[(&str, &str)]) -> BTreeMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn catalogue_sans_doublon_et_premier_champ_public_ou_unique() {
        let ps = platforms();
        let mut ids: Vec<_> = ps.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), ps.len());
        assert!(ps.iter().all(|p| !p.fields.is_empty() && p.fields[0].id == "key"));
    }

    #[test]
    fn saisie_nominale_nettoyee() {
        let ok = validate("bitvavo", &m(&[("key", "  AbCd1234 "), ("secret", "s3cr3t\n")])).unwrap();
        assert_eq!(ok, m(&[("key", "AbCd1234"), ("secret", "s3cr3t")]));
        // Clé privée PEM : plusieurs lignes acceptées.
        let pem = "-----BEGIN EC PRIVATE KEY-----\nMHcCAQ\n-----END EC PRIVATE KEY-----";
        assert!(validate("coinbase", &m(&[("key", "organizations/o/apiKeys/k"), ("secret", pem)])).is_ok());
    }

    #[test]
    fn refus_plateforme_inconnue_champ_absent_vide_inconnu_espace_trop_long() {
        assert_eq!(validate("inconnue", &m(&[])).unwrap_err(), "plateforme inconnue : inconnue");
        assert_eq!(validate("bitvavo", &m(&[("key", "k")])).unwrap_err(), "Bitvavo : « Secret » est vide");
        assert_eq!(validate("okx", &m(&[("key", "k"), ("secret", "s"), ("passphrase", "   ")])).unwrap_err(), "OKX : « Phrase de passe » est vide");
        assert_eq!(validate("kraken", &m(&[("key", "k"), ("secret", "s"), ("pin", "1")])).unwrap_err(), "champ inconnu pour Kraken : pin");
        assert!(validate("kraken", &m(&[("key", "a b"), ("secret", "s")])).unwrap_err().contains("contient un espace"));
        let long = "x".repeat(MAX_FIELD + 1);
        assert!(validate("kraken", &m(&[("key", "k"), ("secret", &long)])).unwrap_err().contains("trop long"));
        // Jumeau accepté : exactement à la borne.
        let at = "x".repeat(MAX_FIELD);
        assert!(validate("kraken", &m(&[("key", "k"), ("secret", &at)])).is_ok());
    }

    #[test]
    fn indice_de_cle_jamais_le_secret() {
        assert_eq!(hint("AbCd1234"), "…1234");
        assert_eq!(hint("ab"), "…ab");
        let a = Accounts::new(Mem::default(), "T");
        let v = a.save("bitvavo", &m(&[("key", "PUBLIC9876"), ("secret", "TRES-SECRET")]), 42).unwrap();
        assert_eq!(v, AccountView { platform: "bitvavo".into(), connected: true, key_hint: Some("…9876".into()), saved_at: Some(42) });
        let json = serde_json::to_string(&a.list().unwrap()).unwrap();
        assert!(!json.contains("TRES-SECRET") && !json.contains("PUBLIC9876"));
        // Clé unique et secrète (One Trading) : aucun indice montré.
        assert_eq!(a.save("onetrading", &m(&[("key", "SECRETE-ENTIERE")]), 1).unwrap().key_hint, None);
    }

    #[test]
    fn remplacer_puis_supprimer() {
        let a = Accounts::new(Mem::default(), "T");
        assert!(!a.view("kraken").unwrap().connected);
        a.save("kraken", &m(&[("key", "k1111"), ("secret", "s")]), 1).unwrap();
        a.save("kraken", &m(&[("key", "k2222"), ("secret", "s")]), 2).unwrap();
        assert_eq!(a.view("kraken").unwrap().key_hint.as_deref(), Some("…2222"));
        assert_eq!(a.credentials("kraken").unwrap().unwrap()["secret"], "s");
        a.delete("kraken").unwrap();
        a.delete("kraken").unwrap(); // supprimer deux fois n'est pas une erreur
        assert!(!a.view("kraken").unwrap().connected);
        assert_eq!(a.list().unwrap().len(), platforms().len());
    }

    #[test]
    fn entree_illisible_signalee() {
        let a = Accounts::new(Mem::default(), "T");
        a.vault.write("T/bitstamp", b"pas du json").unwrap();
        assert!(a.view("bitstamp").unwrap_err().contains("illisible"));
    }

    /// Aller-retour réel dans le Gestionnaire d'identification de Windows, sous un préfixe de test
    /// supprimé à la fin.
    #[cfg(windows)]
    #[test]
    fn gestionnaire_windows_aller_retour() {
        let a = Accounts::new(WindowsVault, "PaperTrading-test-autowin");
        a.delete("bitvavo").unwrap();
        assert!(!a.view("bitvavo").unwrap().connected);
        let v = a.save("bitvavo", &m(&[("key", "TESTKEY0042"), ("secret", "TESTSECRET")]), 7).unwrap();
        assert_eq!(v.key_hint.as_deref(), Some("…0042"));
        assert_eq!(a.credentials("bitvavo").unwrap().unwrap()["secret"], "TESTSECRET");
        a.delete("bitvavo").unwrap();
        assert!(!a.view("bitvavo").unwrap().connected);
    }
}
