//! The RSA keys of KakaoAdHide 1.0.0 – 2.5.0 (.NET XML format), formerly src/keys.rs. Those apps check only the
//! feed's RSA `signature` (written by tools/sign-release.ps1); `release` checks it here before adding the Ed25519
//! one, since the app being released no longer can. Drop this, sign-release.ps1 and the RSA step in build.yml once
//! no copy older than 2.5.1 needs to update.

use base64::Engine;

pub const KEYS: &[&str] = &[
    "<RSAKeyValue><Modulus>18x9wALK9NJHyW9KVlMDNS6G0laUrS0xsdxbLKH+L+4cWAvzRyaCJF83CoUwUvzDsLUJalyeWm9YKm7sYMmKG7Bc9TmUUtY0DSFjKHRN2TxzDq54Gp0ZB96D+s/RpeouE3SAM6dxuknmqqSqXABQQWafD9mObS2u47Ga6lvcSp0olMzMvsmYfHFwNeIXXCfcbNGvecMGn4x60itwQz37mZgM4wU/v+xerKMqA7MadBs6Ygkb/kOUQZ+jpRdP6s7sqnWJAFd3uFMlfn37h1ZcX4zELkV01Y632PU0AIomHlU9zkOaH7514+abc9CjvIiYdOMWQHviBk6Po9ap/u1v8O0OREqUWJ8xK1PVoH+/lORd14zehLebjJnS+YiQpHFVmOY5OHo00/CKGtD03gXKtYiw8+V3Evy47K1d7TCFclHmhQ7rusXRRvT4jWjw7BTGNKneUUUH06uzURYgGnTOeKEWF8jmBGxSNTKZoCaEkev1KdTUCMmwcw15Y61t4J0h</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>",
    "<RSAKeyValue><Modulus>xehnlTXMiZnJha/95EZ0E3PhoK/ZXaa8wnRBn+w0GxAwpSXHw/gyb/07PADPIvQFRIB5obTofGzN9i2baocDLfllxo5nuOyOWLgbMFCnRRtY3N42481vovCiRcW9ysNymx4z1O6w4zZmGyin5kmYZveu1F/XouX0zhPf6BvLpXEPqIH62W0yC/Wvjgn78nxaQktD1O6RcrmR2D/WgQo4GpAo3bwtkC+dwdFMmWb482ndG7kc6pRu+/rsc8yBkxF13zy0ddxLMW3IAV1PmesQbS9CzwjeL/X/ooUSNC2YdBjru6nKoUC+tEX260tedDyher3Y3Oqr5A7kZnTTKKL7sN4JKLtW/k7/yXDNx0wYUynbZfiaoV95WBYjZLblF7Nqr711iXN2qRYKM0qwFnY5S0n9o3M5YXFhXqgHoP3i/t4tT3HSx525r3rB2YEVscoXa4QtHr8emzBkkmgKTy5iijq4HxSpUPKKIRSjJAJzkXKD6SX22vpqu0JUmgnAJ2WN</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>",
];

/// .NET RSA XML public key (<RSAKeyValue><Modulus/><Exponent/></RSAKeyValue>) -> (n, e), big-endian.
fn rsa_xml(xml: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    fn element(xml: &str, name: &str) -> Option<Vec<u8>> {
        let a = xml.find(&format!("<{name}>"))? + name.len() + 2;
        let b = xml[a..].find(&format!("</{name}>"))? + a;
        base64::engine::general_purpose::STANDARD.decode(xml[a..b].trim()).ok()
    }
    Some((element(xml, "Modulus")?, element(xml, "Exponent")?))
}

/// RSA PKCS#1 v1.5 SHA-256 `signature` (base64) over `msg`, with any of KEYS.
pub fn valid(msg: &[u8], signature: &str) -> bool {
    let Ok(sig) = base64::engine::general_purpose::STANDARD.decode(signature.trim()) else { return false };
    KEYS.iter().filter_map(|k| rsa_xml(k)).any(|(n, e)| {
        ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
            .verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256, msg, &sig)
            .is_ok()
    })
}

#[cfg(test)]
mod tests {
    // The published v1.0.0 feed was signed by the release workflow with the primary key.
    #[test]
    fn published_1_0_0_feed_verifies() {
        let v: serde_json::Value = serde_json::from_slice(include_bytes!("../../../tests/fixtures/v1.0.0-latest-windows.json")).unwrap();
        let (ver, sha, sig) = (v["version"].as_str().unwrap(), v["sha256"].as_str().unwrap(), v["signature"].as_str().unwrap());
        assert!(super::valid(&crate::feed::signed_message(ver, sha), sig));
        assert!(!super::valid(&crate::feed::signed_message("1.0.1", sha), sig));
    }
}
