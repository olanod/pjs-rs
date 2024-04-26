use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::js_sys::{Array, Function, Object, Promise, Reflect};

macro_rules! get {
    (^ $obj:expr, $($prop:expr),+ $(,)?) => {{
        let val = get!($obj, $($prop),+);
        val.unchecked_into()
    }};
    ($obj:expr, $($prop:expr),+ $(,)?) => {{
        let mut current_val = JsValue::from($obj);
        $(
            current_val = Reflect::get(&current_val, &JsValue::from_str($prop))
                .unwrap_or_else(|_| panic!("Property '{}' does not exist in {:?}", $prop, current_val));
        )+
        current_val
    }};
}

const NULL: JsValue = JsValue::null();

#[wasm_bindgen]
pub struct PjsExtension {
    pjs: JsValue,
    accounts: Vec<Account>,
    selected: Option<u8>,
}

#[wasm_bindgen]
impl PjsExtension {
    pub async fn connect(app_name: &str) -> Result<PjsExtension, Error> {
        let Some(web3) = web_sys::window().expect("browser").get("injectedWeb3") else {
            return Err(Error::ExtensionUnavailable);
        };
        let pjs = get!(web3, "polkadot-js");
        let enable: Function = get!(^ &pjs, "enable");
        let p = enable
            .call1(&pjs, &app_name.into())
            .expect("promise")
            .unchecked_into::<Promise>();
        let Ok(pjs) = JsFuture::from(p).await else {
            return Err(Error::NoPermission);
        };

        Ok(Self {
            pjs,
            accounts: vec![],
            selected: None,
        })
    }

    #[wasm_bindgen(js_name = account)]
    pub fn current_account(&self) -> Result<Account, Error> {
        let account = self.accounts
            .get(self.selected.ok_or(Error::NoAccountSelected)? as usize)
            .ok_or(Error::NoAccounts)?;
        Ok(account.clone())
    }

    #[wasm_bindgen(js_name = selectAccount)]
    pub fn select_account(&mut self, idx: u8) {
        self.selected = self
            .accounts
            .len()
            .checked_sub(1)
            .map(|i| idx.min(i.min(u8::MAX as usize) as u8));
    }

    ///
    #[wasm_bindgen(js_name = sign)]
    pub async fn js_sign(&self, payload: &str, cb: &Function) -> Result<JsValue, Error> {
        let sign: Function = get!(^ &self.pjs, "signer", "signRaw");
        let account = self
            .accounts
            .get(self.selected.ok_or(Error::NoAccountSelected)? as usize)
            .ok_or(Error::NoAccounts)?;
        let data = {
            let o = Object::new();
            Reflect::set(&o, &"address".into(), &account.address.as_str().into()).unwrap();
            Reflect::set(&o, &"data".into(), &payload.into()).unwrap();
            Reflect::set(&o, &"type".into(), &"bytes".into()).unwrap();
            o
        };

        let p = sign
            .call1(&NULL, &data.into())
            .expect("promise")
            .unchecked_into::<Promise>();
        let signature = JsFuture::from(p).await.map_err(|_| Error::Sign)?;
        let res = cb.call1(&NULL, &signature).map_err(|_| Error::Sign)?;
        Ok(get!(&res, "signature"))
    }

    ///
    #[wasm_bindgen(js_name = fetchAccounts)]
    pub async fn fetch_accounts(&mut self) -> Result<(), Error> {
        let accounts: Function = get!(^ &self.pjs, "accounts", "get");
        let p = accounts.call0(&NULL).unwrap().unchecked_into::<Promise>();
        let Ok(accounts) = JsFuture::from(p).await else {
            return Err(Error::FailedFetchingAccounts);
        };
        self.accounts = Array::from(&accounts)
            .iter()
            .map(|a| {
                let name = get!(&a, "name").as_string().unwrap();
                let address = get!(&a, "address").as_string().unwrap();
                let net: Network = get!(&a, "genesisHash").into();
                Account { name, address, net }
            })
            .collect();
        if !self.accounts.is_empty() {
            self.selected = Some(0);
        }
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn accounts(&self) -> Vec<Account> {
        self.accounts.clone()
    }

    #[wasm_bindgen(getter, js_name = selectedAccount)]
    pub fn get_selected(&self) -> Option<Account> {
        self.selected
            .and_then(|a| self.accounts.get(a as usize))
            .cloned()
    }
}

impl PjsExtension {
    pub async fn sign(&self, payload: &[u8]) -> Result<[u8; 64], Error> {
        let payload = Self::to_hex(payload);
        let mut signature = [0u8; 64];
        let cb = Closure::wrap(Box::new(move |s: JsValue| {
            Self::from_hex(s.as_string().unwrap_or_default().as_str(), &mut signature)
        }) as Box<dyn FnMut(JsValue)>);
        self.js_sign(payload.as_str(), cb.as_ref().unchecked_ref())
            .await?;
        Ok(signature)
    }

    fn to_hex(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(2 + bytes.len());
        let _ = write!(s, "0x");
        for b in bytes {
            let _ = write!(s, "{b:x}");
        }
        s
    }
    fn from_hex(input: &str, buf: &mut [u8]) {
        for (i, b) in buf.iter_mut().enumerate() {
            let Some(s) = input.get(i * 2..i * 2 + 2) else {
                return;
            };
            *b = u8::from_str_radix(s, 16).unwrap_or_default();
        }
    }
}

#[wasm_bindgen]
#[derive(Debug)]
pub enum Error {
    ExtensionUnavailable,
    NoPermission,
    FailedFetchingAccounts,
    NoAccountSelected,
    NoAccounts,
    Sign,
}

#[wasm_bindgen]
#[derive(Debug, Clone)]
pub struct Account {
    name: String,
    address: String,
    net: Network,
}

#[wasm_bindgen]
impl Account {
    #[wasm_bindgen(constructor)]
    pub fn new(name: &str, address: &str, net: Network) -> Self {
        Account {
            name: name.to_string(),
            address: address.to_string(),
            net,
        }
    }
    #[wasm_bindgen(getter)]
    pub fn name(&self) -> String {
        self.name.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn address(&self) -> String {
        self.address.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn network(&self) -> Network {
        self.net
    }
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy)]
pub enum Network {
    Generic,
    Kusama,
    Polkadot,
    Kreivo,
}

const KSM: &str = "0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe";
const DOT: &str = "0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3";
const KREIVO: &str = "0xc710a5f16adc17bcd212cff0aedcbf1c1212a043cdc0fb2dcba861efe5305b01";

impl From<JsValue> for Network {
    fn from(value: JsValue) -> Self {
        let value = value.as_string();
        match value.as_deref() {
            Some(KSM) => Network::Kusama,
            Some(DOT) => Network::Polkadot,
            Some(KREIVO) => Network::Kreivo,
            _ => Network::Generic,
        }
    }
}
