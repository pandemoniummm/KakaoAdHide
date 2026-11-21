// Public keys that update packages must be signed with (RSA, .NET XML format). Written by
// tools/new-signing-keys.ps1. An update is accepted when its signature matches any of these keys.
//   [0] primary: its private key is the SIGNING_PRIVATE_KEY repository secret (used by the release workflow)
//   [1] backup : its private key is kept offline; used only if the primary is lost or has to be replaced
// These are the keys KakaoAdHide 1.0.0 (Windows) shipped with: changing them strands installed copies.

pub const PUBLIC_KEYS: &[&str] = &[
    "<RSAKeyValue><Modulus>18x9wALK9NJHyW9KVlMDNS6G0laUrS0xsdxbLKH+L+4cWAvzRyaCJF83CoUwUvzDsLUJalyeWm9YKm7sYMmKG7Bc9TmUUtY0DSFjKHRN2TxzDq54Gp0ZB96D+s/RpeouE3SAM6dxuknmqqSqXABQQWafD9mObS2u47Ga6lvcSp0olMzMvsmYfHFwNeIXXCfcbNGvecMGn4x60itwQz37mZgM4wU/v+xerKMqA7MadBs6Ygkb/kOUQZ+jpRdP6s7sqnWJAFd3uFMlfn37h1ZcX4zELkV01Y632PU0AIomHlU9zkOaH7514+abc9CjvIiYdOMWQHviBk6Po9ap/u1v8O0OREqUWJ8xK1PVoH+/lORd14zehLebjJnS+YiQpHFVmOY5OHo00/CKGtD03gXKtYiw8+V3Evy47K1d7TCFclHmhQ7rusXRRvT4jWjw7BTGNKneUUUH06uzURYgGnTOeKEWF8jmBGxSNTKZoCaEkev1KdTUCMmwcw15Y61t4J0h</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>",
    "<RSAKeyValue><Modulus>xehnlTXMiZnJha/95EZ0E3PhoK/ZXaa8wnRBn+w0GxAwpSXHw/gyb/07PADPIvQFRIB5obTofGzN9i2baocDLfllxo5nuOyOWLgbMFCnRRtY3N42481vovCiRcW9ysNymx4z1O6w4zZmGyin5kmYZveu1F/XouX0zhPf6BvLpXEPqIH62W0yC/Wvjgn78nxaQktD1O6RcrmR2D/WgQo4GpAo3bwtkC+dwdFMmWb482ndG7kc6pRu+/rsc8yBkxF13zy0ddxLMW3IAV1PmesQbS9CzwjeL/X/ooUSNC2YdBjru6nKoUC+tEX260tedDyher3Y3Oqr5A7kZnTTKKL7sN4JKLtW/k7/yXDNx0wYUynbZfiaoV95WBYjZLblF7Nqr711iXN2qRYKM0qwFnY5S0n9o3M5YXFhXqgHoP3i/t4tT3HSx525r3rB2YEVscoXa4QtHr8emzBkkmgKTy5iijq4HxSpUPKKIRSjJAJzkXKD6SX22vpqu0JUmgnAJ2WN</Modulus><Exponent>AQAB</Exponent></RSAKeyValue>",
];
