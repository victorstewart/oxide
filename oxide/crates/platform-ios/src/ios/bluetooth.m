#import <Foundation/Foundation.h>
#import <CoreBluetooth/CoreBluetooth.h>
#import <uuid/uuid.h>
#import <dispatch/dispatch.h>
#import <stdbool.h>
#import <stdint.h>
#import <stdlib.h>

extern void nametag_host_update_permission(int32_t domain, int32_t status)
   __attribute__((weak_import));
extern void oxide_host_emit_perm(uint32_t domain, uint32_t status)
   __attribute__((weak_import));
extern void oxide_host_ble_emit_state(uint32_t state)
   __attribute__((weak_import));
extern void oxide_host_ble_emit_discovered(const void *info)
   __attribute__((weak_import));
extern void oxide_host_ble_emit_restored(const void *infos, size_t count)
   __attribute__((weak_import));
extern void oxide_host_ble_emit_connected(const uint8_t *addr)
   __attribute__((weak_import));
extern void oxide_host_ble_emit_disconnected(const uint8_t *addr)
   __attribute__((weak_import));
extern void oxide_host_ble_emit_notified(const uint8_t *id,
                                         const uint8_t *service,
                                         const uint8_t *characteristic,
                                         const uint8_t *data,
                                         size_t len)
   __attribute__((weak_import));

static const int32_t kNametagPermissionDomainBluetooth = 4;
static const uint32_t kOxidePermissionDomainBluetooth = 4;
static const int64_t kNametagBleRequestTimeoutNs = 5LL * NSEC_PER_SEC;

typedef __uint128_t nametag_uint128_t;

struct OxideBleScanConfig
{
   const uint8_t *services16;
   size_t service_count;
   uint8_t allow_duplicates;
};

struct OxideBleScanInfo
{
   uint8_t id[16];
   const uint8_t *name_utf8;
   size_t name_len;
   int16_t rssi_dbm;
   const uint8_t *services16;
   size_t service_count;
   const uint8_t *manufacturer_data;
   size_t manufacturer_len;
   uint8_t connectable;
};

struct NametagBleUuid
{
   uint8_t bytes[16];
};

struct NametagBleAdvertisement
{
   const struct NametagBleUuid *services;
   size_t service_count;
   const uint8_t *manufacturer_data;
   size_t manufacturer_len;
   bool connectable;
};

struct NametagBlePeripheral
{
   nametag_uint128_t identifier;
   const uint8_t *name_ptr;
   size_t name_len;
   int16_t rssi_dbm;
   struct NametagBleAdvertisement advertisement;
};

struct NametagBleCacheEntry
{
   struct NametagBlePeripheral peripheral;
   uint64_t last_seen_ms;
};

struct NametagBleNotification
{
   nametag_uint128_t identifier;
   struct NametagBleUuid service;
   struct NametagBleUuid characteristic;
   const uint8_t *data_ptr;
   size_t data_len;
};

struct NametagBleBuffer
{
   const uint8_t *data;
   size_t len;
};

typedef void (*NametagBleEventCallback)(int32_t kind, const void *payload, void *ctx);

static const int32_t NAMETAG_BLE_EVENT_STATE = 0;
static const int32_t NAMETAG_BLE_EVENT_DISCOVERED = 1;
static const int32_t NAMETAG_BLE_EVENT_CONNECTED = 2;
static const int32_t NAMETAG_BLE_EVENT_DISCONNECTED = 3;
static const int32_t NAMETAG_BLE_EVENT_NOTIFICATION = 4;
static const int32_t NAMETAG_BLE_EVENT_CACHE = 5;

static NametagBleEventCallback g_ble_callback = NULL;
static void *g_ble_context = NULL;

static dispatch_queue_t bluetooth_queue(void)
{
   static dispatch_queue_t queue;
   static dispatch_once_t onceToken;
   dispatch_once(&onceToken, ^{
      queue = dispatch_queue_create("com.nametag.bluetooth", DISPATCH_QUEUE_SERIAL);
   });
   return queue;
}

static uint64_t current_time_ms(void)
{
   return (uint64_t)([NSDate date].timeIntervalSince1970 * 1000.0);
}

static int64_t timeout_ns(uint64_t requested_ms)
{
   if (requested_ms == 0)
   {
      return kNametagBleRequestTimeoutNs;
   }
   if (requested_ms >= (uint64_t)(INT64_MAX / (uint64_t)NSEC_PER_MSEC))
   {
      return INT64_MAX;
   }
   return (int64_t)(requested_ms * (uint64_t)NSEC_PER_MSEC);
}

static void uuid_bytes_from_nsuuid(NSUUID *uuid, uint8_t out[16])
{
   if (uuid == nil)
   {
      memset(out, 0, 16);
      return;
   }
   uuid_t bytes;
   [uuid getUUIDBytes:bytes];
   memcpy(out, bytes, 16);
}

static nametag_uint128_t uuid_to_u128(NSUUID *uuid)
{
   uuid_t bytes;
   [uuid getUUIDBytes:bytes];
   nametag_uint128_t value = 0;
   for (NSInteger index = 0; index < 16; index++)
   {
      value <<= 8;
      value |= (nametag_uint128_t)bytes[index];
   }
   return value;
}

static NSUUID *uuid_from_u128(nametag_uint128_t value)
{
   uuid_t bytes;
   for (NSInteger index = 15; index >= 0; index--)
   {
      bytes[index] = (uint8_t)(value & 0xFF);
      value >>= 8;
   }
   return [[NSUUID alloc] initWithUUIDBytes:bytes];
}

static NSUUID *uuid_from_bytes(const uint8_t *bytes)
{
   if (bytes == NULL)
   {
      return nil;
   }
   return [[NSUUID alloc] initWithUUIDBytes:bytes];
}

static CBUUID *uuid_from_struct(const struct NametagBleUuid *uuid)
{
   if (uuid == NULL)
   {
      return nil;
   }
   NSData *data = [NSData dataWithBytes:uuid length:sizeof(*uuid)];
   return [CBUUID UUIDWithData:data];
}

static void fill_nametag_uuid(CBUUID *uuid, struct NametagBleUuid *out)
{
   if (uuid == nil)
   {
      memset(out->bytes, 0, sizeof(out->bytes));
      return;
   }
   NSData *data = uuid.data;
   size_t copy = MIN((size_t)data.length, sizeof(out->bytes));
   memcpy(out->bytes, data.bytes, copy);
   if (copy < sizeof(out->bytes))
   {
      memset(out->bytes + copy, 0, sizeof(out->bytes) - copy);
   }
}

static void fill_oxide_uuid_bytes(CBUUID *uuid, uint8_t out[16])
{
   memset(out, 0, 16);
   if (uuid == nil)
   {
      return;
   }
   NSData *data = uuid.data;
   const uint8_t *bytes = data.bytes;
   if (bytes == NULL)
   {
      return;
   }
   if (data.length >= 16)
   {
      memcpy(out, bytes, 16);
      return;
   }
   if (data.length == 4)
   {
      memcpy(out, bytes, 4);
      return;
   }
   if (data.length == 2)
   {
      static const uint8_t base[16] = {
         0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
         0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB,
      };
      memcpy(out, base, 16);
      out[0] = bytes[0];
      out[1] = bytes[1];
      return;
   }
   memcpy(out, bytes, MIN((NSUInteger)16, data.length));
}

static NSString *characteristic_key(CBUUID *service, CBUUID *characteristic)
{
   return [NSString stringWithFormat:@"%@|%@", service.UUIDString, characteristic.UUIDString];
}

static int32_t nametag_bluetooth_status_code(CBManagerAuthorization authorization)
{
   switch (authorization)
   {
      case CBManagerAuthorizationAllowedAlways:
         return 3;
      case CBManagerAuthorizationDenied:
         return 1;
      case CBManagerAuthorizationRestricted:
         return 1;
      case CBManagerAuthorizationNotDetermined:
      default:
         return 0;
   }
}

static uint32_t oxide_bluetooth_status_code(CBManagerAuthorization authorization)
{
   switch (authorization)
   {
      case CBManagerAuthorizationAllowedAlways:
         return 3;
      case CBManagerAuthorizationDenied:
      case CBManagerAuthorizationRestricted:
         return 1;
      case CBManagerAuthorizationNotDetermined:
      default:
         return 0;
   }
}

static void publish_bluetooth_permission(CBCentralManager *central)
{
   (void)central;
   CBManagerAuthorization authorization = CBManager.authorization;
   int32_t nametag_status = nametag_bluetooth_status_code(authorization);
   if (nametag_host_update_permission != NULL)
   {
      nametag_host_update_permission(kNametagPermissionDomainBluetooth,
                                     nametag_status);
   }
   if (oxide_host_emit_perm != NULL)
   {
      oxide_host_emit_perm(kOxidePermissionDomainBluetooth,
                           oxide_bluetooth_status_code(authorization));
   }
}

static void emit_oxide_state_event(BOOL powered)
{
   if (oxide_host_ble_emit_state != NULL)
   {
      oxide_host_ble_emit_state(powered ? 1 : 0);
   }
}

static void emit_oxide_connected_event(NSUUID *identifier)
{
   if (oxide_host_ble_emit_connected == NULL)
   {
      return;
   }
   uint8_t bytes[16];
   uuid_bytes_from_nsuuid(identifier, bytes);
   oxide_host_ble_emit_connected(bytes);
}

static void emit_oxide_disconnected_event(NSUUID *identifier)
{
   if (oxide_host_ble_emit_disconnected == NULL)
   {
      return;
   }
   uint8_t bytes[16];
   uuid_bytes_from_nsuuid(identifier, bytes);
   oxide_host_ble_emit_disconnected(bytes);
}

static void emit_oxide_notification_event(CBPeripheral *peripheral,
                                          CBCharacteristic *characteristic,
                                          NSData *data)
{
   if (oxide_host_ble_emit_notified == NULL || data.length == 0)
   {
      return;
   }
   uint8_t id[16];
   uint8_t service[16];
   uint8_t chr[16];
   uuid_bytes_from_nsuuid(peripheral.identifier, id);
   fill_oxide_uuid_bytes(characteristic.service.UUID, service);
   fill_oxide_uuid_bytes(characteristic.UUID, chr);
   oxide_host_ble_emit_notified(id, service, chr, data.bytes, data.length);
}

static NSData *oxide_service_bytes_from_advertisement(NSDictionary *advertisement)
{
   NSArray<CBUUID *> *services = advertisement[CBAdvertisementDataServiceUUIDsKey];
   if (services.count == 0)
   {
      return [NSData data];
   }
   NSMutableData *bytes = [NSMutableData dataWithLength:services.count * 16];
   uint8_t *cursor = bytes.mutableBytes;
   for (NSUInteger index = 0; index < services.count; index++)
   {
      fill_oxide_uuid_bytes(services[index], cursor + (index * 16));
   }
   return bytes;
}

static void emit_oxide_discovered_event(CBPeripheral *peripheral,
                                        NSData *nameData,
                                        NSDictionary *advertisement,
                                        int16_t rssi)
{
   if (oxide_host_ble_emit_discovered == NULL)
   {
      return;
   }

   NSData *serviceBytes = oxide_service_bytes_from_advertisement(advertisement);
   NSData *manufacturer = advertisement[CBAdvertisementDataManufacturerDataKey];
   NSNumber *connectable = advertisement[CBAdvertisementDataIsConnectable];
   struct OxideBleScanInfo info;
   memset(&info, 0, sizeof(info));
   uuid_bytes_from_nsuuid(peripheral.identifier, info.id);
   info.name_utf8 = (const uint8_t *)nameData.bytes;
   info.name_len = nameData.length;
   info.rssi_dbm = rssi;
   info.services16 = (const uint8_t *)serviceBytes.bytes;
   info.service_count = serviceBytes.length / 16;
   info.manufacturer_data = (const uint8_t *)manufacturer.bytes;
   info.manufacturer_len = manufacturer.length;
   info.connectable = connectable.boolValue ? 1 : 0;
   oxide_host_ble_emit_discovered(&info);
}

static void emit_oxide_restored_event(NSArray<CBPeripheral *> *peripherals)
{
   if (oxide_host_ble_emit_restored == NULL || peripherals.count == 0)
   {
      return;
   }

   struct OxideBleScanInfo *infos =
       calloc((size_t)peripherals.count, sizeof(struct OxideBleScanInfo));
   if (infos == NULL)
   {
      return;
   }

   NSMutableArray<NSData *> *nameData =
       [[NSMutableArray alloc] initWithCapacity:peripherals.count];
   for (NSUInteger index = 0; index < peripherals.count; index++)
   {
      CBPeripheral *peripheral = peripherals[index];
      NSData *encodedName = nil;
      if (peripheral.name.length > 0)
      {
         encodedName = [peripheral.name dataUsingEncoding:NSUTF8StringEncoding];
      }
      if (encodedName == nil)
      {
         encodedName = [NSData data];
      }
      [nameData addObject:encodedName];
      uuid_bytes_from_nsuuid(peripheral.identifier, infos[index].id);
      infos[index].name_utf8 = (const uint8_t *)encodedName.bytes;
      infos[index].name_len = encodedName.length;
      infos[index].connectable = 1;
   }

   oxide_host_ble_emit_restored(infos, peripherals.count);
   free(infos);
}

static void emit_state_event(BOOL powered)
{
   emit_oxide_state_event(powered);
   if (g_ble_callback != NULL)
   {
      bool state = powered;
      g_ble_callback(NAMETAG_BLE_EVENT_STATE, &state, g_ble_context);
   }
}

static void emit_connected_event(NSUUID *identifier)
{
   emit_oxide_connected_event(identifier);
   if (g_ble_callback != NULL)
   {
      nametag_uint128_t value = uuid_to_u128(identifier);
      g_ble_callback(NAMETAG_BLE_EVENT_CONNECTED, &value, g_ble_context);
   }
}

static void emit_disconnected_event(NSUUID *identifier)
{
   emit_oxide_disconnected_event(identifier);
   if (g_ble_callback != NULL)
   {
      nametag_uint128_t value = uuid_to_u128(identifier);
      g_ble_callback(NAMETAG_BLE_EVENT_DISCONNECTED, &value, g_ble_context);
   }
}

static void emit_notification_event(CBPeripheral *peripheral, CBCharacteristic *characteristic, NSData *data)
{
   emit_oxide_notification_event(peripheral, characteristic, data);
   if (g_ble_callback == NULL || data.length == 0)
   {
      return;
   }
   struct NametagBleNotification payload;
   memset(&payload, 0, sizeof(payload));
   payload.identifier = uuid_to_u128(peripheral.identifier);
   fill_nametag_uuid(characteristic.service.UUID, &payload.service);
   fill_nametag_uuid(characteristic.UUID, &payload.characteristic);
   payload.data_ptr = data.bytes;
   payload.data_len = data.length;
   g_ble_callback(NAMETAG_BLE_EVENT_NOTIFICATION, &payload, g_ble_context);
}

static void emit_peripheral_snapshot(int32_t kind,
   CBPeripheral *peripheral,
   NSData *nameData,
   NSData *serviceBytes,
   NSDictionary *advertisement,
   int16_t rssi,
   uint64_t lastSeenMs)
{
   if (kind == NAMETAG_BLE_EVENT_DISCOVERED)
   {
      emit_oxide_discovered_event(peripheral, nameData, advertisement, rssi);
   }
   if (g_ble_callback == NULL)
   {
      return;
   }

   struct NametagBlePeripheral payload;
   memset(&payload, 0, sizeof(payload));
   payload.identifier = uuid_to_u128(peripheral.identifier);
   payload.rssi_dbm = rssi;

   if (nameData.length > 0)
   {
      payload.name_ptr = (const uint8_t *)nameData.bytes;
      payload.name_len = nameData.length;
   }

   if (serviceBytes.length > 0)
   {
      payload.advertisement.services = (const struct NametagBleUuid *)serviceBytes.bytes;
      payload.advertisement.service_count = serviceBytes.length / sizeof(struct NametagBleUuid);
   }

   NSData *manufacturer = advertisement[CBAdvertisementDataManufacturerDataKey];
   if (manufacturer.length > 0)
   {
      payload.advertisement.manufacturer_data = manufacturer.bytes;
      payload.advertisement.manufacturer_len = manufacturer.length;
   }
   NSNumber *connectable = advertisement[CBAdvertisementDataIsConnectable];
   payload.advertisement.connectable = connectable.boolValue;

   if (kind == NAMETAG_BLE_EVENT_CACHE)
   {
      struct NametagBleCacheEntry cache;
      memset(&cache, 0, sizeof(cache));
      cache.peripheral = payload;
      cache.last_seen_ms = lastSeenMs;
      g_ble_callback(kind, &cache, g_ble_context);
      return;
   }

   g_ble_callback(kind, &payload, g_ble_context);
}

@interface NametagReadRequest : NSObject
@property(nonatomic, strong) dispatch_semaphore_t semaphore;
@property(nonatomic, strong, nullable) NSData *data;
@property(nonatomic, strong, nullable) NSError *error;
@end

@implementation NametagReadRequest
@end

@interface NametagWriteRequest : NSObject
@property(nonatomic, strong) dispatch_semaphore_t semaphore;
@property(nonatomic, strong, nullable) NSError *error;
@end

@implementation NametagWriteRequest
@end

@interface NametagPeripheralState : NSObject
@property(nonatomic, strong) CBPeripheral *peripheral;
@property(nonatomic, strong) NSDictionary *advertisement;
@property(nonatomic, assign) int16_t rssiDbm;
@property(nonatomic, assign) uint64_t lastSeenMs;
@property(nonatomic, strong) NSData *nameData;
@property(nonatomic, strong) NSData *serviceUUIDBytes;
@property(nonatomic, strong) NSMutableDictionary<NSString *, CBCharacteristic *> *characteristics;
@property(nonatomic, strong) NSMutableDictionary<NSString *, NametagReadRequest *> *pendingReads;
@property(nonatomic, strong) NSMutableDictionary<NSString *, NametagWriteRequest *> *pendingWrites;
@end

@implementation NametagPeripheralState

- (instancetype)init
{
   self = [super init];
   if (!self)
   {
      return nil;
   }
   _characteristics = [[NSMutableDictionary alloc] init];
   _pendingReads = [[NSMutableDictionary alloc] init];
   _pendingWrites = [[NSMutableDictionary alloc] init];
   return self;
}

- (void)resetPendingRequestsWithError:(NSError *)error
{
   for (NametagReadRequest *request in self.pendingReads.allValues)
   {
      request.error = error;
      if (request.semaphore != NULL)
      {
         dispatch_semaphore_signal(request.semaphore);
      }
   }
   [self.pendingReads removeAllObjects];

   for (NametagWriteRequest *request in self.pendingWrites.allValues)
   {
      request.error = error;
      if (request.semaphore != NULL)
      {
         dispatch_semaphore_signal(request.semaphore);
      }
   }
   [self.pendingWrites removeAllObjects];
}

@end

@interface NametagBluetoothBridge : NSObject <CBCentralManagerDelegate, CBPeripheralDelegate, CBPeripheralManagerDelegate>
@property(nonatomic, strong) CBCentralManager *central;
@property(nonatomic, strong) NSMutableDictionary<NSUUID *, NametagPeripheralState *> *records;
@property(nonatomic, strong) NSMutableSet<NSUUID *> *connecting;
@property(nonatomic, strong) CBPeripheralManager *peripheralManager;
@property(nonatomic, strong) NSString *restoreIdentifier;
@property(nonatomic, assign) BOOL showPowerAlert;

- (instancetype)initWithRestoreIdentifier:(NSString *)restoreIdentifier
   showPowerAlert:(BOOL)showPowerAlert;

- (void)startScanWithServices:(NSArray<CBUUID *> *)services allowDuplicates:(BOOL)allowDuplicates;
- (void)stopScan;

- (BOOL)readCharacteristicForPeripheral:(NSUUID *)identifier
   service:(CBUUID *)service
   characteristic:(CBUUID *)characteristic
   request:(NametagReadRequest *)request;

- (BOOL)writeCharacteristicForPeripheral:(NSUUID *)identifier
   service:(CBUUID *)service
   characteristic:(CBUUID *)characteristic
   data:(NSData *)data
   withResponse:(BOOL)withResponse
   request:(NametagWriteRequest *)request;

- (BOOL)setNotifyForPeripheral:(NSUUID *)identifier
   service:(CBUUID *)service
   characteristic:(CBUUID *)characteristic
   enable:(BOOL)enable;

- (void)cancelPendingReadForService:(CBUUID *)service
   characteristic:(CBUUID *)characteristic
   request:(NametagReadRequest *)request;

- (void)cancelPendingWriteForService:(CBUUID *)service
   characteristic:(CBUUID *)characteristic
   request:(NametagWriteRequest *)request;

@end

@implementation NametagBluetoothBridge

- (instancetype)initWithRestoreIdentifier:(NSString *)restoreIdentifier
   showPowerAlert:(BOOL)showPowerAlert
{
   self = [super init];
   if (!self)
   {
      return nil;
   }
   _records = [[NSMutableDictionary alloc] init];
   _connecting = [[NSMutableSet alloc] init];
   _restoreIdentifier = [restoreIdentifier copy] ?: @"";
   _showPowerAlert = showPowerAlert;
   NSMutableDictionary *options = [[NSMutableDictionary alloc] init];
   options[CBCentralManagerOptionShowPowerAlertKey] = @(showPowerAlert);
   if (_restoreIdentifier.length > 0)
   {
      options[CBCentralManagerOptionRestoreIdentifierKey] = _restoreIdentifier;
   }
   _central = [[CBCentralManager alloc] initWithDelegate:self queue:bluetooth_queue() options:options];
   publish_bluetooth_permission(_central);
   return self;
}

- (BOOL)isPoweredOn
{
   return self.central.state == CBManagerStatePoweredOn;
}

- (NametagPeripheralState *)stateForPeripheral:(CBPeripheral *)peripheral create:(BOOL)create
{
   if (peripheral.identifier == nil)
   {
      return nil;
   }
   NametagPeripheralState *state = self.records[peripheral.identifier];
   if (!state && create)
   {
      state = [[NametagPeripheralState alloc] init];
      self.records[peripheral.identifier] = state;
   }
   if (state)
   {
      state.peripheral = peripheral;
   }
   return state;
}

- (void)startScanWithServices:(NSArray<CBUUID *> *)services allowDuplicates:(BOOL)allowDuplicates
{
   NSMutableDictionary *options = [[NSMutableDictionary alloc] init];
   options[CBCentralManagerScanOptionAllowDuplicatesKey] = @(allowDuplicates);
   [self.central scanForPeripheralsWithServices:services options:options];
}

- (void)stopScan
{
   [self.central stopScan];
}

- (CBCharacteristic *)characteristicForState:(NametagPeripheralState *)state service:(CBUUID *)service characteristic:(CBUUID *)characteristic
{
   NSString *key = characteristic_key(service, characteristic);
   CBCharacteristic *target = state.characteristics[key];
   if (target)
   {
      return target;
   }

   for (CBService *srv in state.peripheral.services)
   {
      if ([srv.UUID isEqual:service])
      {
         for (CBCharacteristic *chr in srv.characteristics)
         {
            if ([chr.UUID isEqual:characteristic])
            {
               state.characteristics[key] = chr;
               return chr;
            }
         }
      }
   }
   return nil;
}

- (void)ensureCharacteristicsForPeripheral:(NametagPeripheralState *)state service:(CBUUID *)service
{
   if (state.peripheral.state != CBPeripheralStateConnected)
   {
      return;
   }
   NSArray<CBService *> *services = state.peripheral.services;
   BOOL discovered = NO;
   for (CBService *srv in services)
   {
      if ([srv.UUID isEqual:service])
      {
         discovered = YES;
         if (srv.characteristics.count == 0)
         {
            [state.peripheral discoverCharacteristics:nil forService:srv];
         }
         break;
      }
   }
   if (!discovered)
   {
      [state.peripheral discoverServices:@[ service ]];
   }
}

- (BOOL)readCharacteristicForPeripheral:(NSUUID *)identifier service:(CBUUID *)service characteristic:(CBUUID *)characteristic request:(NametagReadRequest *)request
{
   NametagPeripheralState *state = self.records[identifier];
   if (!state || state.peripheral.state != CBPeripheralStateConnected)
   {
      return NO;
   }
   CBCharacteristic *target = [self characteristicForState:state service:service characteristic:characteristic];
   if (!target)
   {
      [self ensureCharacteristicsForPeripheral:state service:service];
      return NO;
   }
   NSString *key = characteristic_key(service, characteristic);
   state.pendingReads[key] = request;
   [state.peripheral readValueForCharacteristic:target];
   return YES;
}

- (BOOL)writeCharacteristicForPeripheral:(NSUUID *)identifier
   service:(CBUUID *)service
   characteristic:(CBUUID *)characteristic
   data:(NSData *)data
   withResponse:(BOOL)withResponse
   request:(NametagWriteRequest *)request
{
   NametagPeripheralState *state = self.records[identifier];
   if (!state || state.peripheral.state != CBPeripheralStateConnected)
   {
      return NO;
   }
   CBCharacteristic *target = [self characteristicForState:state service:service characteristic:characteristic];
   if (!target)
   {
      [self ensureCharacteristicsForPeripheral:state service:service];
      return NO;
   }

   CBCharacteristicProperties properties = target.properties;
   if (withResponse)
   {
      if (!(properties & CBCharacteristicPropertyWrite))
      {
         return NO;
      }
      NSString *key = characteristic_key(service, characteristic);
      state.pendingWrites[key] = request;
      [state.peripheral writeValue:data forCharacteristic:target type:CBCharacteristicWriteWithResponse];
   }
   else
   {
      if (!(properties & CBCharacteristicPropertyWriteWithoutResponse))
      {
         return NO;
      }
      [state.peripheral writeValue:data forCharacteristic:target type:CBCharacteristicWriteWithoutResponse];
   }
   return YES;
}

- (BOOL)setNotifyForPeripheral:(NSUUID *)identifier service:(CBUUID *)service characteristic:(CBUUID *)characteristic enable:(BOOL)enable
{
   NametagPeripheralState *state = self.records[identifier];
   if (!state || state.peripheral.state != CBPeripheralStateConnected)
   {
      return NO;
   }
   CBCharacteristic *target = [self characteristicForState:state service:service characteristic:characteristic];
   if (!target)
   {
      [self ensureCharacteristicsForPeripheral:state service:service];
      return NO;
   }
   if (!(target.properties & CBCharacteristicPropertyNotify))
   {
      return NO;
   }
   [state.peripheral setNotifyValue:enable forCharacteristic:target];
   return YES;
}

- (void)cancelPendingReadForService:(CBUUID *)service characteristic:(CBUUID *)characteristic request:(NametagReadRequest *)request
{
   for (NametagPeripheralState *state in self.records.allValues)
   {
      NSString *key = characteristic_key(service, characteristic);
      NametagReadRequest *current = state.pendingReads[key];
      if (current == request)
      {
         [state.pendingReads removeObjectForKey:key];
         break;
      }
   }
}

- (void)cancelPendingWriteForService:(CBUUID *)service characteristic:(CBUUID *)characteristic request:(NametagWriteRequest *)request
{
   for (NametagPeripheralState *state in self.records.allValues)
   {
      NSString *key = characteristic_key(service, characteristic);
      NametagWriteRequest *current = state.pendingWrites[key];
      if (current == request)
      {
         [state.pendingWrites removeObjectForKey:key];
         break;
      }
   }
}

- (void)centralManagerDidUpdateState:(CBCentralManager *)central
{
   emit_state_event(central.state == CBManagerStatePoweredOn);
   publish_bluetooth_permission(central);
}

- (void)centralManager:(CBCentralManager *)central
   didDiscoverPeripheral:(CBPeripheral *)peripheral
   advertisementData:(NSDictionary<NSString *, id> *)advertisementData
   RSSI:(NSNumber *)RSSI
{
   int16_t rssi = 0;
   if (RSSI != nil)
   {
      rssi = (int16_t)RSSI.integerValue;
   }
   NametagPeripheralState *state = [self stateForPeripheral:peripheral create:YES];
   state.advertisement = advertisementData ?: @{};
   state.rssiDbm = rssi;
   state.lastSeenMs = current_time_ms();

   NSString *name = peripheral.name;
   if (name.length == 0)
   {
      NSString *local = advertisementData[CBAdvertisementDataLocalNameKey];
      if (local.length > 0)
      {
         name = local;
      }
   }
   if (name.length > 0)
   {
      state.nameData = [name dataUsingEncoding:NSUTF8StringEncoding];
   }
   else
   {
      state.nameData = [NSData data];
   }

   NSArray<CBUUID *> *services = advertisementData[CBAdvertisementDataServiceUUIDsKey];
   if (services.count > 0)
   {
      NSMutableData *serviceBytes = [NSMutableData dataWithLength:services.count * sizeof(struct NametagBleUuid)];
      struct NametagBleUuid *out = (struct NametagBleUuid *)serviceBytes.mutableBytes;
      for (NSUInteger index = 0; index < services.count; index++)
      {
         fill_nametag_uuid(services[index], &out[index]);
      }
      state.serviceUUIDBytes = serviceBytes;
   }
   else
   {
      state.serviceUUIDBytes = [NSData data];
   }

   emit_peripheral_snapshot(NAMETAG_BLE_EVENT_DISCOVERED,
      peripheral,
      state.nameData,
      state.serviceUUIDBytes,
      state.advertisement,
      rssi,
      state.lastSeenMs);
   emit_peripheral_snapshot(NAMETAG_BLE_EVENT_CACHE,
      peripheral,
      state.nameData,
      state.serviceUUIDBytes,
      state.advertisement,
      rssi,
      state.lastSeenMs);
}

- (void)centralManager:(CBCentralManager *)central didConnectPeripheral:(CBPeripheral *)peripheral
{
   [self.connecting removeObject:peripheral.identifier];
   peripheral.delegate = self;
   [peripheral discoverServices:nil];
   emit_connected_event(peripheral.identifier);
}

- (void)centralManager:(CBCentralManager *)central didFailToConnectPeripheral:(CBPeripheral *)peripheral error:(NSError *)error
{
   NametagPeripheralState *state = [self stateForPeripheral:peripheral create:NO];
   [self.connecting removeObject:peripheral.identifier];
   [state resetPendingRequestsWithError:error];
   emit_disconnected_event(peripheral.identifier);
}

- (void)centralManager:(CBCentralManager *)central didDisconnectPeripheral:(CBPeripheral *)peripheral error:(NSError *)error
{
   NametagPeripheralState *state = [self stateForPeripheral:peripheral create:NO];
   [self.connecting removeObject:peripheral.identifier];
   [state resetPendingRequestsWithError:error];
   emit_disconnected_event(peripheral.identifier);
}

- (void)centralManager:(CBCentralManager *)central
   willRestoreState:(NSDictionary<NSString *, id> *)state
{
   NSArray<CBPeripheral *> *peripherals =
       state[CBCentralManagerRestoredStatePeripheralsKey];
   if (peripherals.count == 0)
   {
      return;
   }
   for (CBPeripheral *peripheral in peripherals)
   {
      NametagPeripheralState *record = [self stateForPeripheral:peripheral
                                                         create:YES];
      record.peripheral = peripheral;
      peripheral.delegate = self;
   }
   emit_oxide_restored_event(peripherals);
}

- (void)peripheral:(CBPeripheral *)peripheral didDiscoverServices:(NSError *)error
{
   if (error)
   {
      return;
   }
   for (CBService *service in peripheral.services)
   {
      [peripheral discoverCharacteristics:nil forService:service];
   }
}

- (void)peripheral:(CBPeripheral *)peripheral didDiscoverCharacteristicsForService:(CBService *)service error:(NSError *)error
{
   NametagPeripheralState *state = [self stateForPeripheral:peripheral create:NO];
   if (error || !state)
   {
      return;
   }
   for (CBCharacteristic *characteristic in service.characteristics)
   {
      NSString *key = characteristic_key(service.UUID, characteristic.UUID);
      state.characteristics[key] = characteristic;
   }
}

- (void)peripheral:(CBPeripheral *)peripheral didUpdateValueForCharacteristic:(CBCharacteristic *)characteristic error:(NSError *)error
{
   NametagPeripheralState *state = [self stateForPeripheral:peripheral create:NO];
   if (!state)
   {
      return;
   }
   NSString *key = characteristic_key(characteristic.service.UUID, characteristic.UUID);
   NametagReadRequest *request = state.pendingReads[key];
   if (request)
   {
      if (!error && characteristic.value.length > 0)
      {
         request.data = [characteristic.value copy];
      }
      request.error = error;
      [state.pendingReads removeObjectForKey:key];
      if (request.semaphore != NULL)
      {
         dispatch_semaphore_signal(request.semaphore);
      }
   }

   if (!error && characteristic.value.length > 0)
   {
      emit_notification_event(peripheral, characteristic, characteristic.value);
   }
}

- (void)peripheral:(CBPeripheral *)peripheral didWriteValueForCharacteristic:(CBCharacteristic *)characteristic error:(NSError *)error
{
   NametagPeripheralState *state = [self stateForPeripheral:peripheral create:NO];
   if (!state)
   {
      return;
   }
   NSString *key = characteristic_key(characteristic.service.UUID, characteristic.UUID);
   NametagWriteRequest *request = state.pendingWrites[key];
   if (!request)
   {
      return;
   }
   request.error = error;
   [state.pendingWrites removeObjectForKey:key];
   if (request.semaphore != NULL)
   {
      dispatch_semaphore_signal(request.semaphore);
   }
}

- (void)ensurePeripheralManager
{
   if (self.peripheralManager != nil)
   {
      return;
   }
   NSMutableDictionary *options = [[NSMutableDictionary alloc] init];
   options[CBPeripheralManagerOptionShowPowerAlertKey] = @(self.showPowerAlert);
   if (self.restoreIdentifier.length > 0)
   {
      options[CBPeripheralManagerOptionRestoreIdentifierKey] =
          self.restoreIdentifier;
   }
   self.peripheralManager = [[CBPeripheralManager alloc]
       initWithDelegate:self
                  queue:bluetooth_queue()
                options:options];
}

- (void)peripheralManagerDidUpdateState:(CBPeripheralManager *)peripheral
{
   (void)peripheral;
}

- (void)startAdvertisingWithName:(NSString *)name services:(NSArray<CBUUID *> *)services
{
   [self ensurePeripheralManager];
   if (self.peripheralManager.state != CBManagerStatePoweredOn)
   {
      return;
   }
   NSMutableDictionary *payload = [[NSMutableDictionary alloc] init];
   if (name.length > 0)
   {
      payload[CBAdvertisementDataLocalNameKey] = name;
   }
   if (services.count > 0)
   {
      payload[CBAdvertisementDataServiceUUIDsKey] = services;
   }
   [self.peripheralManager startAdvertising:payload];
}

- (void)stopAdvertising
{
   if (self.peripheralManager != nil)
   {
      [self.peripheralManager stopAdvertising];
   }
}

@end

static NametagBluetoothBridge *ensure_bridge(NSString *restoreIdentifier,
                                             BOOL showPowerAlert)
{
   static NametagBluetoothBridge *instance = nil;
   static dispatch_once_t onceToken;
   dispatch_once(&onceToken, ^{
      instance = [[NametagBluetoothBridge alloc]
          initWithRestoreIdentifier:restoreIdentifier
                    showPowerAlert:showPowerAlert];
   });
   return instance;
}

static NametagBluetoothBridge *bridge(void)
{
   return ensure_bridge(nil, YES);
}

bool nametag_ios_bluetooth_powered_on(void)
{
   __block BOOL powered = NO;
   dispatch_sync(bluetooth_queue(), ^{
      powered = [bridge() isPoweredOn];
   });
   return powered;
}

void nametag_ios_bluetooth_subscribe(NametagBleEventCallback cb, void *ctx)
{
   dispatch_async(bluetooth_queue(), ^{
      g_ble_callback = cb;
      g_ble_context = ctx;
      emit_state_event([bridge() isPoweredOn]);
      publish_bluetooth_permission(bridge().central);
   });
}

static NSArray<CBUUID *> *uuids_from_host(const struct NametagBleUuid *uuids, size_t count)
{
   if (uuids == NULL || count == 0)
   {
      return @[];
   }
   NSMutableArray<CBUUID *> *list = [[NSMutableArray alloc] initWithCapacity:count];
   for (size_t index = 0; index < count; index++)
   {
      NSData *data = [NSData dataWithBytes:&uuids[index] length:sizeof(struct NametagBleUuid)];
      [list addObject:[CBUUID UUIDWithData:data]];
   }
   return list;
}

void nametag_ios_bluetooth_start_scan(const struct NametagBleUuid *services, size_t count, bool allow_duplicates)
{
   NSArray<CBUUID *> *uuids = uuids_from_host(services, count);
   dispatch_async(bluetooth_queue(), ^{
      [bridge() startScanWithServices:uuids allowDuplicates:allow_duplicates];
   });
}

void nametag_ios_bluetooth_stop_scan(void)
{
   dispatch_async(bluetooth_queue(), ^{
      [bridge() stopScan];
   });
}

static BOOL bluetooth_read_uuid(NSUUID *uuid,
                                CBUUID *serviceUuid,
                                CBUUID *charUuid,
                                uint64_t timeoutMs,
                                struct NametagBleBuffer *out)
{
   if (out != NULL)
   {
      out->data = NULL;
      out->len = 0;
   }

   dispatch_semaphore_t wait = dispatch_semaphore_create(0);
   __block NametagReadRequest *request = [[NametagReadRequest alloc] init];
   request.semaphore = wait;
   __block BOOL started = NO;

   dispatch_sync(bluetooth_queue(), ^{
      started = [bridge() readCharacteristicForPeripheral:uuid
                                                  service:serviceUuid
                                           characteristic:charUuid
                                                  request:request];
   });

   if (!started)
   {
      return NO;
   }

   long result = dispatch_semaphore_wait(
       wait,
       dispatch_time(DISPATCH_TIME_NOW, timeout_ns(timeoutMs)));
   if (result != 0)
   {
      dispatch_sync(bluetooth_queue(), ^{
         [bridge() cancelPendingReadForService:serviceUuid
                                characteristic:charUuid
                                       request:request];
      });
      return NO;
   }

   if (request.error != nil || request.data.length == 0)
   {
      return NO;
   }

   uint8_t *buffer = malloc(request.data.length);
   if (buffer == NULL)
   {
      return NO;
   }
   memcpy(buffer, request.data.bytes, request.data.length);
   if (out != NULL)
   {
      out->data = buffer;
      out->len = request.data.length;
   }
   else
   {
      free(buffer);
   }
   return YES;
}

static BOOL bluetooth_write_uuid(NSUUID *uuid,
                                 CBUUID *serviceUuid,
                                 CBUUID *charUuid,
                                 NSData *payload,
                                 BOOL withResponse,
                                 uint64_t timeoutMs)
{
   dispatch_semaphore_t wait =
       withResponse ? dispatch_semaphore_create(0) : NULL;
   __block NametagWriteRequest *request = nil;
   if (withResponse)
   {
      request = [[NametagWriteRequest alloc] init];
      request.semaphore = wait;
   }

   __block BOOL started = NO;
   dispatch_sync(bluetooth_queue(), ^{
      started = [bridge() writeCharacteristicForPeripheral:uuid
                                                   service:serviceUuid
                                            characteristic:charUuid
                                                      data:payload
                                              withResponse:withResponse
                                                   request:request];
   });

   if (!started)
   {
      return NO;
   }

   if (!withResponse)
   {
      return YES;
   }

   long result = dispatch_semaphore_wait(
       wait,
       dispatch_time(DISPATCH_TIME_NOW, timeout_ns(timeoutMs)));
   if (result != 0)
   {
      dispatch_sync(bluetooth_queue(), ^{
         [bridge() cancelPendingWriteForService:serviceUuid
                                 characteristic:charUuid
                                        request:request];
      });
      return NO;
   }
   return request.error == nil;
}

bool nametag_ios_bluetooth_connect(nametag_uint128_t identifier)
{
   NSUUID *uuid = uuid_from_u128(identifier);
   __block BOOL result = NO;
   dispatch_sync(bluetooth_queue(), ^{
      NametagBluetoothBridge *br = bridge();
      NametagPeripheralState *state = br.records[uuid];
      CBPeripheral *peripheral = state.peripheral;
      if (!peripheral)
      {
         NSArray<CBPeripheral *> *retrieved = [br.central retrievePeripheralsWithIdentifiers:@[ uuid ]];
         peripheral = retrieved.firstObject;
      }
      if (peripheral)
      {
         state = [br stateForPeripheral:peripheral create:YES];
         state.peripheral = peripheral;
         peripheral.delegate = br;
         [br.connecting addObject:uuid];
         [br.central connectPeripheral:peripheral options:nil];
         result = YES;
      }
   });
   return result;
}

bool nametag_ios_bluetooth_disconnect(nametag_uint128_t identifier)
{
   NSUUID *uuid = uuid_from_u128(identifier);
   __block BOOL result = NO;
   dispatch_sync(bluetooth_queue(), ^{
      NametagBluetoothBridge *br = bridge();
      NametagPeripheralState *state = br.records[uuid];
      if (state.peripheral)
      {
         [br.central cancelPeripheralConnection:state.peripheral];
         result = YES;
      }
   });
   return result;
}

bool nametag_ios_bluetooth_read(nametag_uint128_t identifier,
   const struct NametagBleUuid *service,
   const struct NametagBleUuid *characteristic,
   struct NametagBleBuffer *out)
{
   NSUUID *uuid = uuid_from_u128(identifier);
   CBUUID *serviceUuid = uuid_from_struct(service);
   CBUUID *charUuid = uuid_from_struct(characteristic);
   return bluetooth_read_uuid(uuid, serviceUuid, charUuid, 0, out);
}

bool nametag_ios_bluetooth_write(nametag_uint128_t identifier,
   const struct NametagBleUuid *service,
   const struct NametagBleUuid *characteristic,
   const uint8_t *data,
   size_t len,
   bool with_response)
{
   if (len > 0 && data == NULL)
   {
      return false;
   }
   NSData *payload = [NSData dataWithBytes:data length:len];
   NSUUID *uuid = uuid_from_u128(identifier);
   CBUUID *serviceUuid = uuid_from_struct(service);
   CBUUID *charUuid = uuid_from_struct(characteristic);
   return bluetooth_write_uuid(uuid, serviceUuid, charUuid, payload,
                               with_response, 0);
}

bool nametag_ios_bluetooth_notify(nametag_uint128_t identifier,
   const struct NametagBleUuid *service,
   const struct NametagBleUuid *characteristic,
   bool enable)
{
   NSUUID *uuid = uuid_from_u128(identifier);
   CBUUID *serviceUuid = uuid_from_struct(service);
   CBUUID *charUuid = uuid_from_struct(characteristic);
   __block BOOL result = NO;
   dispatch_sync(bluetooth_queue(), ^{
      result = [bridge() setNotifyForPeripheral:uuid service:serviceUuid characteristic:charUuid enable:enable];
   });
   return result;
}

bool nametag_ios_bluetooth_advertise_start(const uint8_t *name_ptr,
   size_t name_len,
   const struct NametagBleUuid *services,
   size_t count)
{
   NSString *name = nil;
   if (name_ptr != NULL && name_len > 0)
   {
      name = [[NSString alloc] initWithBytes:name_ptr length:name_len encoding:NSUTF8StringEncoding];
   }
   NSArray<CBUUID *> *uuids = uuids_from_host(services, count);
   dispatch_async(bluetooth_queue(), ^{
      [bridge() startAdvertisingWithName:name services:uuids];
   });
   return true;
}

void nametag_ios_bluetooth_advertise_stop(void)
{
   dispatch_async(bluetooth_queue(), ^{
      [bridge() stopAdvertising];
   });
}

size_t nametag_ios_bluetooth_copy_cache(struct NametagBleCacheEntry *out, size_t capacity)
{
   __block NSArray<NametagPeripheralState *> *states = nil;
   dispatch_sync(bluetooth_queue(), ^{
      states = bridge().records.allValues;
   });
   if (out == NULL || capacity == 0)
   {
      return states.count;
   }
   size_t written = MIN((size_t)states.count, capacity);
   for (size_t index = 0; index < written; index++)
   {
      NametagPeripheralState *state = states[index];
      CBPeripheral *peripheral = state.peripheral;
      emit_peripheral_snapshot(NAMETAG_BLE_EVENT_CACHE,
         peripheral,
         state.nameData,
         state.serviceUUIDBytes,
         state.advertisement,
         state.rssiDbm,
         state.lastSeenMs);
      struct NametagBleCacheEntry entry;
      memset(&entry, 0, sizeof(entry));
      entry.peripheral.identifier = uuid_to_u128(peripheral.identifier);
      entry.peripheral.rssi_dbm = state.rssiDbm;
      if (state.nameData.length > 0)
      {
         entry.peripheral.name_ptr = (const uint8_t *)state.nameData.bytes;
         entry.peripheral.name_len = state.nameData.length;
      }
      if (state.serviceUUIDBytes.length > 0)
      {
         entry.peripheral.advertisement.services = (const struct NametagBleUuid *)state.serviceUUIDBytes.bytes;
         entry.peripheral.advertisement.service_count = state.serviceUUIDBytes.length / sizeof(struct NametagBleUuid);
      }
      NSData *manufacturer = state.advertisement[CBAdvertisementDataManufacturerDataKey];
      if (manufacturer.length > 0)
      {
         entry.peripheral.advertisement.manufacturer_data = manufacturer.bytes;
         entry.peripheral.advertisement.manufacturer_len = manufacturer.length;
      }
      NSNumber *connectable = state.advertisement[CBAdvertisementDataIsConnectable];
      entry.peripheral.advertisement.connectable = connectable.boolValue;
      entry.last_seen_ms = state.lastSeenMs;
      out[index] = entry;
   }
   return written;
}

void nametag_ios_bluetooth_buffer_free(struct NametagBleBuffer buffer)
{
   if (buffer.data != NULL)
   {
      free((void *)buffer.data);
   }
}

uint8_t oxide_ble_is_supported(void)
{
   return 1;
}

void oxide_ble_init_with_restoration(const char *restore_id)
{
   NSString *identifier = nil;
   if (restore_id != NULL)
   {
      identifier = [NSString stringWithUTF8String:restore_id];
   }
   dispatch_sync(bluetooth_queue(), ^{
      (void)ensure_bridge(identifier, NO);
   });
}

void oxide_ble_init(void)
{
   oxide_ble_init_with_restoration(NULL);
}

uint8_t oxide_ble_powered_on(void)
{
   __block BOOL powered = NO;
   dispatch_sync(bluetooth_queue(), ^{
      powered = [ensure_bridge(nil, NO) isPoweredOn];
   });
   return powered ? 1 : 0;
}

void oxide_ble_shutdown(void)
{
   dispatch_sync(bluetooth_queue(), ^{
      NametagBluetoothBridge *shared = ensure_bridge(nil, NO);
      [shared stopScan];
      [shared stopAdvertising];
   });
}

void oxide_ble_start_scan(const struct OxideBleScanConfig *cfg)
{
   const struct NametagBleUuid *services =
       cfg != NULL ? (const struct NametagBleUuid *)cfg->services16 : NULL;
   size_t count = cfg != NULL ? cfg->service_count : 0;
   bool allowDuplicates =
       cfg != NULL ? cfg->allow_duplicates != 0 : false;
   NSArray<CBUUID *> *uuids = uuids_from_host(services, count);
   dispatch_async(bluetooth_queue(), ^{
      [ensure_bridge(nil, NO) startScanWithServices:uuids
                                    allowDuplicates:allowDuplicates];
   });
}

void oxide_ble_stop_scan(void)
{
   dispatch_async(bluetooth_queue(), ^{
      [ensure_bridge(nil, NO) stopScan];
   });
}

void oxide_ble_connect(const uint8_t *id16)
{
   if (id16 == NULL)
   {
      return;
   }
   __block NSUUID *uuid = uuid_from_bytes(id16);
   dispatch_sync(bluetooth_queue(), ^{
      NametagBluetoothBridge *br = ensure_bridge(nil, NO);
      NametagPeripheralState *state = br.records[uuid];
      CBPeripheral *peripheral = state.peripheral;
      if (peripheral == nil)
      {
         NSArray<CBPeripheral *> *retrieved =
             [br.central retrievePeripheralsWithIdentifiers:@[ uuid ]];
         peripheral = retrieved.firstObject;
      }
      if (peripheral != nil)
      {
         state = [br stateForPeripheral:peripheral create:YES];
         state.peripheral = peripheral;
         peripheral.delegate = br;
         [br.connecting addObject:uuid];
         [br.central connectPeripheral:peripheral options:nil];
      }
   });
}

void oxide_ble_disconnect(const uint8_t *id16)
{
   if (id16 == NULL)
   {
      return;
   }
   __block NSUUID *uuid = uuid_from_bytes(id16);
   dispatch_sync(bluetooth_queue(), ^{
      NametagPeripheralState *state = ensure_bridge(nil, NO).records[uuid];
      if (state.peripheral != nil)
      {
         [ensure_bridge(nil, NO).central cancelPeripheralConnection:state.peripheral];
      }
   });
}

int oxide_ble_read(const uint8_t *id16,
                   const uint8_t *service16,
                   const uint8_t *characteristic16,
                   uint8_t **out_ptr,
                   size_t *out_len,
                   uint32_t timeout_ms)
{
   if (out_ptr != NULL)
   {
      *out_ptr = NULL;
   }
   if (out_len != NULL)
   {
      *out_len = 0;
   }
   if (id16 == NULL || service16 == NULL || characteristic16 == NULL)
   {
      return 0;
   }

   struct NametagBleBuffer buffer;
   memset(&buffer, 0, sizeof(buffer));
   BOOL ok = bluetooth_read_uuid(uuid_from_bytes(id16),
                                 [CBUUID UUIDWithData:[NSData dataWithBytes:service16
                                                                       length:16]],
                                 [CBUUID UUIDWithData:[NSData dataWithBytes:characteristic16
                                                                       length:16]],
                                 timeout_ms,
                                 &buffer);
   if (!ok)
   {
      return 0;
   }
   if (out_ptr != NULL)
   {
      *out_ptr = (uint8_t *)buffer.data;
   }
   if (out_len != NULL)
   {
      *out_len = buffer.len;
   }
   return 1;
}

int oxide_ble_write(const uint8_t *id16,
                    const uint8_t *service16,
                    const uint8_t *characteristic16,
                    const uint8_t *data,
                    size_t len,
                    uint8_t with_response,
                    uint32_t timeout_ms)
{
   if (id16 == NULL || service16 == NULL || characteristic16 == NULL)
   {
      return 0;
   }
   if (len > 0 && data == NULL)
   {
      return 0;
   }
   NSData *payload = [NSData dataWithBytes:data length:len];
   BOOL ok = bluetooth_write_uuid(
       uuid_from_bytes(id16),
       [CBUUID UUIDWithData:[NSData dataWithBytes:service16 length:16]],
       [CBUUID UUIDWithData:[NSData dataWithBytes:characteristic16
                                           length:16]],
       payload,
       with_response != 0,
       timeout_ms);
   return ok ? 1 : 0;
}

int oxide_ble_notify(const uint8_t *id16,
                     const uint8_t *service16,
                     const uint8_t *characteristic16,
                     uint8_t enable,
                     uint32_t timeout_ms)
{
   (void)timeout_ms;
   if (id16 == NULL || service16 == NULL || characteristic16 == NULL)
   {
      return 0;
   }
   __block BOOL result = NO;
   dispatch_sync(bluetooth_queue(), ^{
      result = [ensure_bridge(nil, NO)
          setNotifyForPeripheral:uuid_from_bytes(id16)
                         service:[CBUUID UUIDWithData:[NSData dataWithBytes:service16
                                                                       length:16]]
                  characteristic:[CBUUID UUIDWithData:[NSData dataWithBytes:characteristic16
                                                                       length:16]]
                          enable:enable != 0];
   });
   return result ? 1 : 0;
}

void oxide_ble_advertise_start(const char *name, const uint8_t *service_uuid)
{
   NSString *advertisedName = nil;
   if (name != NULL)
   {
      advertisedName = [NSString stringWithUTF8String:name];
   }
   NSArray<CBUUID *> *services = @[];
   if (service_uuid != NULL)
   {
      services = @[ [CBUUID UUIDWithData:[NSData dataWithBytes:service_uuid
                                                        length:16]] ];
   }
   dispatch_async(bluetooth_queue(), ^{
      [ensure_bridge(nil, NO) startAdvertisingWithName:advertisedName
                                              services:services];
   });
}

void oxide_ble_advertise_stop(void)
{
   dispatch_async(bluetooth_queue(), ^{
      [ensure_bridge(nil, NO) stopAdvertising];
   });
}
