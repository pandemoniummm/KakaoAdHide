// Public keys that update feeds must be signed with (Ed25519, base64). Made with tools/feed-sign (keygen). An
// update is accepted when its signature matches any of these keys.
//   [0] primary: its private key is the SIGNING_KEY_ED25519 repository secret (used by the release workflow)
//   [1] backup : its private key is kept offline; used only if the primary is lost or has to be replaced
// Changing them strands installed copies: a new key goes in one release before anything is signed with it.
// (1.0.0 – 2.5.0 used RSA keys; they reach 2.5.2 through the bridge feed in packaging/bridge/.)

pub const PUBLIC_KEYS: &[&str] = &[
    "T8jXSm6RZGSnUY0A+I55sAre/kWnY4jNrrkloOsg21c=",
    "a8RZLSCDr0wbmaltpvg9AflaJzR25ZXstux3slmjIQs=",
];
