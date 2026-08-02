#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <limits.h>
#import <stdio.h>

#include "../../src/ios/network.m"

static NSString *const ROOT_DER_BASE64 =
   @"MIIDLTCCAhWgAwIBAgIUN0j8A5r1V79/tPRm71uxj/OpZgMwDQYJKoZIhvcNAQELBQAwHjEcMBoGA1UEAwwTT3hpZGUgVExT"
    "IFRlc3QgUm9vdDAeFw0yNjA4MDIyMTI4NDhaFw0zNjA3MzAyMTI4NDhaMB4xHDAaBgNVBAMME094aWRlIFRMUyBUZXN0IFJv"
    "b3QwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCcaw/twDim5uQUPXYHW7Cg9sUs9CecK7HFy+DdZONIgdcYIjXk"
    "92s3tpY3RCbwQ39HZPiQfhYEkf2Qw/BDimfUswTz612+f+74sXWQknz/XSArxih5lwN9RNk+n+lvZMdSyUNtqkRtEwfUZP76"
    "5/7VTf/KlJqGs54CL8Hpi90/BAi7Q4NaTnDiYcjmjtcqgAoIm/PE9z2mnC7p+r9/Zg84lmfW475f+NoLR0raZa5lb6kLhEqu"
    "sja3KXcmpHMXZmuS86XH6A3a7Qze8NYxfizI++QCsGYEYjn19rz8UPAcR1tA93spAr5r76Q2Tbbz1WNH/i1BOCmSue2jctIH"
    "Ah2XAgMBAAGjYzBhMB0GA1UdDgQWBBQBj/WmwAWuTk1mbeOuvsxHL4zwQDAfBgNVHSMEGDAWgBQBj/WmwAWuTk1mbeOuvsxH"
    "L4zwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjANBgkqhkiG9w0BAQsFAAOCAQEAOQD993JTYznRXqyptsZf"
    "LRUMm8xeRpGgCYzYZb3hS2vh1foUpwGuiCr28xn3U1sbZoDY3B60pv5+Ndrtv9a4LTFFm5+ab//2+IApKFZ3skOhKpd8a/fR"
    "Mgx87kljWTu3/FSOqYs943eYHNauCa13xiGsMsMHd2/42LgIjm1z7G6+fkSrrX3uXuOR7qfXhU1a2DpnSMIFc++T0310gHX2"
    "25Uy8l4o1ZJtmJBQMzPEf3TjU/V1WanfayeOdbkkCpNij9SBLBcZLbuzju8IyeECdUqLF5QL7zy9OcEM1dAfPVHQmLllQdoE"
    "u1VMGb6IWPB7rD/U8z8ARC5xRrDuOglcWQ==";

static NSString *const LEAF_DER_BASE64 =
   @"MIIDYTCCAkmgAwIBAgIUTxvLwHYIzTeERIEfHoe2bpACzOwwDQYJKoZIhvcNAQELBQAwHjEcMBoGA1UEAwwTT3hpZGUgVExT"
    "IFRlc3QgUm9vdDAeFw0yNjA4MDIyMTMzNTNaFw0yNzA4MDIyMTMzNTNaMB4xHDAaBgNVBAMME2V4cGVjdGVkLm94aWRlLnRl"
    "c3QwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCOGulXFthU5OGzK+Q2DA+lQBNuBDEFY/pT3PDKCo8opFQX80aa"
    "IL5DpQKcDn+qR9xtigj/eP8yVDPqmBsEVors/7HPR9+y69qTxbFNwZJgwaHJttlDIFtrHHQqqdmh0ZcPzxPZ+6KqVG84h9En"
    "hvb2txoanm4UIGta4/OYXgDB6cdimbiRyEwK2JdYh240J2eOQpyomp82ZHyIkKVg8fejxUp3YK0FvryhIOkBbW4wcBv6R1MN"
    "gOILnEusatHEkPWDtqPdKkd8veQIWoByXpj63QJU5agHFcbQZRV7R+ust/D7vIlV5dnjZ7bXWAcX8tArxDoJxB6ki/bCr4hT"
    "s7+PAgMBAAGjgZYwgZMwDAYDVR0TAQH/BAIwADAOBgNVHQ8BAf8EBAMCBaAwEwYDVR0lBAwwCgYIKwYBBQUHAwEwHgYDVR0R"
    "BBcwFYITZXhwZWN0ZWQub3hpZGUudGVzdDAdBgNVHQ4EFgQUq0IhHuT45OryG6lsRz5wMOwKk4cwHwYDVR0jBBgwFoAUAY/1"
    "psAFrk5NZm3jrr7MRy+M8EAwDQYJKoZIhvcNAQELBQADggEBABrEP9AgAgcK0/edWq+am9Pca+HAOWGhVu55bvYKm/9piMtw"
    "Azj0e5o+dAsZQk4uDzYhpM4VMeNGu5EEsMWBosCKw1AwzM2vFtMWanyEE+36202nFCJ9VXOtWeKFcF1BiI3Slp38kdtdI9Ao"
    "ZOIR273QXkHQs7tvCgROXrJ0ywMx69kzL3PHC5SOk1+X6KEm+Kx9yAAsRhUeULzu2idwEyKLzzna6Si9S6ohMWYzR2xjvuV+"
    "+NXxyvp1iBHyOYRjWVHrZIVDPYWqevAGP7hoF4o8wbu7HCTje13s0d+em7gYtoGKA5nx480JfZNG+VOPIVx4y6JGWZkSCzmG"
    "Ytzl4GQ=";

static int failures = 0;

static void check(BOOL condition, const char *label)
{
   if (!condition)
   {
      fprintf(stderr, "FAIL: %s\n", label);
      failures += 1;
   }
}

static NSData *decode(NSString *base64)
{
   return [[NSData alloc] initWithBase64EncodedString:base64 options:0];
}

static NSArray *parse_anchors(const struct OxideTlsTrustAnchor *anchors, size_t count)
{
   struct OxideQuicTlsConfig tls = {0};
   tls.trust_anchors = anchors;
   tls.trust_anchor_count = count;
   return copy_trust_anchors(&tls);
}

static BOOL evaluate_leaf(NSData *leaf_data, NSString *hostname, NSArray *anchors,
                          size_t anchor_count, BOOL enforce_hostname)
{
   SecCertificateRef leaf = SecCertificateCreateWithData(
       kCFAllocatorDefault, (__bridge CFDataRef)leaf_data);
   if (leaf == NULL)
   {
      return NO;
   }
   SecPolicyRef policy = SecPolicyCreateSSL(
       true, (__bridge CFStringRef)hostname);
   if (policy == NULL)
   {
      CFRelease(leaf);
      return NO;
   }
   SecTrustRef trust = NULL;
   OSStatus status = SecTrustCreateWithCertificates(leaf, policy, &trust);
   CFRelease(policy);
   CFRelease(leaf);
   if (status != errSecSuccess || trust == NULL)
   {
      return NO;
   }
   NSDate *verify_date = [NSDate dateWithTimeIntervalSince1970:1785792945.0];
   if (SecTrustSetVerifyDate(trust, (__bridge CFDateRef)verify_date) != errSecSuccess)
   {
      CFRelease(trust);
      return NO;
   }
   BOOL result = evaluate_sec_trust(
       trust, anchors, anchor_count, enforce_hostname);
   CFRelease(trust);
   return result;
}

int main(void)
{
   @autoreleasepool
   {
      NSData *root = decode(ROOT_DER_BASE64);
      NSData *leaf = decode(LEAF_DER_BASE64);
      check(root != nil && leaf != nil, "fixture decode");

      struct OxideTlsTrustAnchor valid = {
         .data = root.bytes,
         .len = root.length,
      };
      NSArray *anchors = parse_anchors(&valid, 1);
      check(anchors.count == 1, "exact DER anchor parses");
      check(evaluate_leaf(leaf, @"expected.oxide.test", anchors, 1, YES),
            "matching hostname and exact anchor");
      check(!evaluate_leaf(leaf, @"wrong.oxide.test", anchors, 1, YES),
            "wrong hostname is rejected");
      check(evaluate_leaf(leaf, @"wrong.oxide.test", anchors, 1, NO),
            "hostname suppression preserves chain trust");
      check(!evaluate_leaf(leaf, @"wrong.oxide.test", nil, 0, NO),
            "hostname suppression rejects an untrusted chain");
      check(!evaluate_leaf(leaf, @"expected.oxide.test", anchors, 2, YES),
            "anchor count mismatch is rejected");
      check(!evaluate_sec_trust(NULL, anchors, 1, YES),
            "null trust is rejected");

      struct OxideTlsTrustAnchor null_anchor = {
         .data = NULL,
         .len = root.length,
      };
      check(parse_anchors(&null_anchor, 1) == nil, "null anchor is rejected");

      struct OxideTlsTrustAnchor empty = {
         .data = root.bytes,
         .len = 0,
      };
      check(parse_anchors(&empty, 1) == nil, "empty anchor is rejected");

      struct OxideTlsTrustAnchor oversized = {
         .data = root.bytes,
         .len = (size_t)LONG_MAX + 1u,
      };
      check(parse_anchors(&oversized, 1) == nil, "oversized anchor is rejected");

      const uint8_t malformed_bytes[] = {0x30, 0x03, 0x01, 0x01, 0xff};
      struct OxideTlsTrustAnchor malformed = {
         .data = malformed_bytes,
         .len = sizeof(malformed_bytes),
      };
      check(parse_anchors(&malformed, 1) == nil, "malformed DER is rejected");

      NSMutableData *trailing_data = [root mutableCopy];
      const uint8_t trailing_byte = 0;
      [trailing_data appendBytes:&trailing_byte length:1];
      struct OxideTlsTrustAnchor trailing = {
         .data = trailing_data.bytes,
         .len = trailing_data.length,
      };
      check(parse_anchors(&trailing, 1) == nil,
            "trailing non-canonical DER is rejected");

      const struct OxideTlsTrustAnchor mixed[] = {valid, malformed};
      check(parse_anchors(mixed, 2) == nil,
            "mixed valid and invalid anchors reject atomically");
   }
   if (failures == 0)
   {
      printf("native TLS trust checks passed\n");
   }
   return failures == 0 ? 0 : 1;
}
