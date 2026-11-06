# Run against a fresh local Worker using these test-only credentials.
# The final request intentionally leaves the local client locked for 10 minutes.
$ErrorActionPreference = 'Stop'
$base = 'http://127.0.0.1:8791/api/admin'
$credentials = @{ email = 'test@example.com'; password = 'local-test-password' }

function Login($body) {
    Invoke-WebRequest "$base/login" -Method Post -ContentType 'application/json' `
        -Body ($body | ConvertTo-Json) -SkipHttpErrorCheck
}

$response = Login $credentials
if ($response.StatusCode -ne 200) { throw "Valid login returned $($response.StatusCode)" }
$session = $response.Content | ConvertFrom-Json
$response = Invoke-WebRequest "$base/me" -Headers @{ Authorization = "Bearer $($session.token)" }
if (-not ($response.Content | ConvertFrom-Json).authenticated) { throw 'Session was rejected' }

$response = Login @{ email = $credentials.email; password = 'wrong-test-only' }
if ($response.StatusCode -ne 401) { throw "Wrong password returned $($response.StatusCode)" }
$response = Login $credentials
if ($response.StatusCode -ne 429 -or [int]$response.Headers['Retry-After'][0] -notin 590..600) {
    throw 'A failed login did not apply the 10-minute lock'
}
Write-Output 'Passed login, session verification, wrong password, and lockout checks.'
