#import <Foundation/Foundation.h>
#import <Security/Security.h>
#import <stdint.h>
#import <stdlib.h>
#import <string.h>

static NSString *oxide_secure_storage_service(void)
{
   return @"com.oxide.secure-storage";
}

static NSMutableDictionary *oxide_secure_storage_query(NSData *account)
{
   return [@{
      (__bridge id)kSecClass : (__bridge id)kSecClassGenericPassword,
      (__bridge id)kSecAttrService : oxide_secure_storage_service(),
      (__bridge id)kSecAttrAccount : account,
   } mutableCopy];
}

int32_t oxide_secure_storage_save(const uint8_t *key_ptr,
                                  size_t key_len,
                                  const uint8_t *data_ptr,
                                  size_t data_len)
{
   if (key_ptr == NULL || key_len == 0 || (data_ptr == NULL && data_len > 0))
   {
      return (int32_t)errSecParam;
   }

   NSData *account = [NSData dataWithBytes:key_ptr length:key_len];
   NSData *payload = data_len == 0 ? [NSData data] : [NSData dataWithBytes:data_ptr length:data_len];
   NSMutableDictionary *query = oxide_secure_storage_query(account);
   SecItemDelete((__bridge CFDictionaryRef)query);
   query[(__bridge id)kSecValueData] = payload;
   OSStatus status = SecItemAdd((__bridge CFDictionaryRef)query, NULL);
   return status == errSecSuccess ? 0 : (int32_t)status;
}

int32_t oxide_secure_storage_load(const uint8_t *key_ptr,
                                  size_t key_len,
                                  const uint8_t **out_data_ptr,
                                  size_t *out_data_len)
{
   if (out_data_ptr != NULL)
   {
      *out_data_ptr = NULL;
   }
   if (out_data_len != NULL)
   {
      *out_data_len = 0;
   }
   if (key_ptr == NULL || key_len == 0 || out_data_ptr == NULL || out_data_len == NULL)
   {
      return (int32_t)errSecParam;
   }

   NSData *account = [NSData dataWithBytes:key_ptr length:key_len];
   NSMutableDictionary *query = oxide_secure_storage_query(account);
   query[(__bridge id)kSecReturnData] = @YES;
   query[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
   CFTypeRef result = NULL;
   OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)query, &result);
   if (status == errSecItemNotFound)
   {
      return 1;
   }
   if (status != errSecSuccess)
   {
      return (int32_t)status;
   }

   NSData *data = CFBridgingRelease(result);
   if (![data isKindOfClass:NSData.class])
   {
      return (int32_t)errSecInternalComponent;
   }
   if (data.length == 0)
   {
      return 0;
   }

   uint8_t *copy = (uint8_t *)malloc(data.length);
   if (copy == NULL)
   {
      return (int32_t)errSecAllocate;
   }
   memcpy(copy, data.bytes, data.length);
   *out_data_ptr = copy;
   *out_data_len = (size_t)data.length;
   return 0;
}

int32_t oxide_secure_storage_delete(const uint8_t *key_ptr, size_t key_len)
{
   if (key_ptr == NULL || key_len == 0)
   {
      return (int32_t)errSecParam;
   }

   NSData *account = [NSData dataWithBytes:key_ptr length:key_len];
   NSMutableDictionary *query = oxide_secure_storage_query(account);
   OSStatus status = SecItemDelete((__bridge CFDictionaryRef)query);
   if (status == errSecItemNotFound)
   {
      return 1;
   }
   return status == errSecSuccess ? 0 : (int32_t)status;
}

void oxide_secure_storage_free_data(const uint8_t *data_ptr, size_t data_len)
{
   (void)data_len;
   if (data_ptr != NULL)
   {
      free((void *)data_ptr);
   }
}
