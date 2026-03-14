#import <CoreBluetooth/CoreBluetooth.h>
#import <Foundation/Foundation.h>

// ===== Structs =====

typedef struct {
  const uint8_t *services16;
  size_t service_count;
  uint8_t allow_duplicates;
} OxBleScanConfig;

typedef struct {
  uint8_t id16[16];
  const uint8_t *name_utf8;
  size_t name_len;
  int16_t rssi_dbm;
  const uint8_t *services16;
  size_t service_count;
  const uint8_t *manufacturer_data;
  size_t manufacturer_len;
  uint8_t connectable;
} OxBleScanInfo;

// ===== FFI Declarations =====

void oxide_host_ble_emit_state(uint32_t state);
void oxide_host_ble_emit_discovered(const void *info);
void oxide_host_ble_emit_connected(const uint8_t *addr);
void oxide_host_ble_emit_disconnected(const uint8_t *addr);
void oxide_host_ble_emit_restored(const void *infos, size_t count);

// ===== Helpers =====

static void UUIDBytesFromNSUUID(NSUUID *uuid, uint8_t out[16]) {
  uuid_t bytes;
  [uuid getUUIDBytes:bytes];
  memcpy(out, bytes, 16);
}

static void FillCBUUIDBytes(CBUUID *uuid, uint8_t out[16]) {
  memset(out, 0, 16);
  NSData *data = uuid.data;
  if (!data)
    return;
  const uint8_t *bytes = data.bytes;
  if (!bytes)
    return;
  if (data.length >= 16) {
    memcpy(out, bytes, 16);
  } else if (data.length == 4) {
    memcpy(out, bytes, 4);
  } else if (data.length == 2) {
    // Embed 16-bit UUID into Bluetooth base UUID (little-endian)
    static const uint8_t base[16] = {0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                                     0x10, 0x00, 0x80, 0x00, 0x00, 0x80,
                                     0x5F, 0x9B, 0x34, 0xFB};
    memcpy(out, base, 16);
    out[0] = bytes[0];
    out[1] = bytes[1];
  } else {
    memcpy(out, bytes, data.length);
  }
}

static NSArray<CBUUID *> *BleServicesFromConfig(const OxBleScanConfig *cfg) {
  if (!cfg || cfg->service_count == 0 || cfg->services16 == NULL) {
    return nil;
  }
  NSMutableArray<CBUUID *> *services =
      [NSMutableArray arrayWithCapacity:cfg->service_count];
  for (size_t i = 0; i < cfg->service_count; ++i) {
    const uint8_t *ptr = cfg->services16 + (i * 16);
    NSData *data = [NSData dataWithBytes:ptr length:16];
    CBUUID *uuid = [CBUUID UUIDWithData:data];
    if (uuid)
      [services addObject:uuid];
  }
  return services.count ? services : nil;
}

// ===== Bluetooth Manager =====

@interface OxBleManager
    : NSObject <CBCentralManagerDelegate, CBPeripheralDelegate,
                CBPeripheralManagerDelegate>
@property(nonatomic, strong) CBCentralManager *central;
@property(nonatomic, strong) CBPeripheralManager *peripheralManager;
@property(nonatomic, strong)
    NSMutableDictionary<NSUUID *, CBPeripheral *> *peripherals;
@property(nonatomic, strong)
    NSMutableDictionary<NSUUID *, CBCharacteristic *> *characteristics;
@end

static OxBleManager *s_ble_manager = nil;

static OxBleManager *BleContext(void) {
  NSCAssert(s_ble_manager != nil, @"BleContext accessed before initialization");
  return s_ble_manager;
}

@implementation OxBleManager

- (instancetype)initWithRestoreIdentifier:(NSString *)restoreId {
  self = [super init];
  if (self) {
    _peripherals = [NSMutableDictionary dictionary];
    _characteristics = [NSMutableDictionary dictionary];

    NSMutableDictionary *centralOpts = [NSMutableDictionary dictionary];
    centralOpts[CBCentralManagerOptionShowPowerAlertKey] = @NO;
    if (restoreId) {
      centralOpts[CBCentralManagerOptionRestoreIdentifierKey] = restoreId;
    }

    _central = [[CBCentralManager alloc]
        initWithDelegate:self
                   queue:dispatch_get_main_queue()
                 options:centralOpts];

    NSMutableDictionary *peripheralOpts = [NSMutableDictionary dictionary];
    peripheralOpts[CBPeripheralManagerOptionShowPowerAlertKey] = @NO;
    if (restoreId) {
      peripheralOpts[CBPeripheralManagerOptionRestoreIdentifierKey] = restoreId;
    }

    _peripheralManager = [[CBPeripheralManager alloc]
        initWithDelegate:self
                   queue:dispatch_get_main_queue()
                 options:peripheralOpts];
  }
  return self;
}

// --- CBCentralManagerDelegate ---

- (void)centralManagerDidUpdateState:(CBCentralManager *)central {
  oxide_host_ble_emit_state((central.state == CBManagerStatePoweredOn) ? 1
                                                                         : 0);
}

- (void)centralManager:(CBCentralManager *)central
    didDiscoverPeripheral:(CBPeripheral *)peripheral
        advertisementData:(NSDictionary<NSString *, id> *)advertisementData
                     RSSI:(NSNumber *)RSSI {
  if (!peripheral.identifier) {
    return;
  }

  self.peripherals[peripheral.identifier] = peripheral;

  uint8_t id_bytes[16];
  UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);

  NSString *name = peripheral.name.length ? peripheral.name : nil;
  if (!name) {
    name = advertisementData[CBAdvertisementDataLocalNameKey];
  }
  NSData *nameData =
      name.length ? [name dataUsingEncoding:NSUTF8StringEncoding] : nil;

  NSArray<CBUUID *> *serviceUUIDs =
      advertisementData[CBAdvertisementDataServiceUUIDsKey];
  NSMutableData *serviceData = nil;
  if (serviceUUIDs.count) {
    serviceData = [NSMutableData dataWithLength:serviceUUIDs.count * 16];
    uint8_t *dst = serviceData.mutableBytes;
    NSUInteger idx = 0;
    for (CBUUID *uuid in serviceUUIDs) {
      uint8_t buf[16];
      FillCBUUIDBytes(uuid, buf);
      memcpy(dst + idx * 16, buf, 16);
      idx += 1;
    }
  }

  NSData *manufacturer =
      advertisementData[CBAdvertisementDataManufacturerDataKey];
  NSNumber *connectable = advertisementData[CBAdvertisementDataIsConnectable];

  OxBleScanInfo info = {
      .name_utf8 = (const uint8_t *)nameData.bytes,
      .name_len = nameData.length,
      .rssi_dbm = (int16_t)RSSI.intValue,
      .services16 = (const uint8_t *)serviceData.bytes,
      .service_count = serviceUUIDs.count,
      .manufacturer_data = (const uint8_t *)manufacturer.bytes,
      .manufacturer_len = manufacturer.length,
      .connectable = connectable ? connectable.boolValue : 0,
  };
  memcpy(info.id16, id_bytes, 16);

  oxide_host_ble_emit_discovered(&info);
}

- (void)centralManager:(CBCentralManager *)central
    didConnectPeripheral:(CBPeripheral *)peripheral {
  uint8_t id_bytes[16];
  UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
  oxide_host_ble_emit_connected(id_bytes);
  peripheral.delegate = self;
  [peripheral discoverServices:nil];
}

- (void)centralManager:(CBCentralManager *)central
    didDisconnectPeripheral:(CBPeripheral *)peripheral
                      error:(NSError *)error {
  uint8_t id_bytes[16];
  UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
  oxide_host_ble_emit_disconnected(id_bytes);
}

- (void)centralManager:(CBCentralManager *)central
    didFailToConnectPeripheral:(CBPeripheral *)peripheral
                         error:(NSError *)error {
  uint8_t id_bytes[16];
  UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
  oxide_host_ble_emit_disconnected(id_bytes);
}

- (void)centralManager:(CBCentralManager *)central
      willRestoreState:(NSDictionary<NSString *, id> *)dict {
  NSLog(@"[Oxide] CBCentralManager willRestoreState");
  NSArray<CBPeripheral *> *restoredPeripherals =
      dict[CBCentralManagerRestoredStatePeripheralsKey];
  if (restoredPeripherals.count == 0) {
    return;
  }

  size_t count = restoredPeripherals.count;
  OxBleScanInfo *infos = calloc(count, sizeof(OxBleScanInfo));
  if (!infos)
    return;

  for (NSUInteger i = 0; i < count; i++) {
    CBPeripheral *peripheral = restoredPeripherals[i];

    self.peripherals[peripheral.identifier] = peripheral;
    peripheral.delegate = self;

    uint8_t id_bytes[16];
    UUIDBytesFromNSUUID(peripheral.identifier, id_bytes);
    memcpy(infos[i].id16, id_bytes, 16);

    NSString *name = peripheral.name;
    NSData *nameData =
        name.length ? [name dataUsingEncoding:NSUTF8StringEncoding] : nil;

    infos[i].name_utf8 = (const uint8_t *)nameData.bytes;
    infos[i].name_len = nameData.length;
    infos[i].rssi_dbm = 0;
    infos[i].services16 = NULL;
    infos[i].service_count = 0;
    infos[i].manufacturer_data = NULL;
    infos[i].manufacturer_len = 0;
    infos[i].connectable = 1;
  }

  oxide_host_ble_emit_restored(infos, count);
  free(infos);
}

// --- CBPeripheralDelegate ---

- (void)peripheral:(CBPeripheral *)peripheral
    didDiscoverServices:(NSError *)error {
  if (error)
    return;
  for (CBService *service in peripheral.services) {
    [peripheral discoverCharacteristics:nil forService:service];
  }
}

- (void)peripheral:(CBPeripheral *)peripheral
    didDiscoverCharacteristicsForService:(CBService *)service
                                   error:(NSError *)error {
  if (error)
    return;
  for (CBCharacteristic *c in service.characteristics) {
    self.characteristics[[CBUUID UUIDWithData:c.UUID.data].UUIDString] = c;
    if ((c.properties & CBCharacteristicPropertyNotify)) {
      [peripheral setNotifyValue:YES forCharacteristic:c];
    }
  }
}

- (void)peripheral:(CBPeripheral *)peripheral
    didUpdateValueForCharacteristic:(CBCharacteristic *)characteristic
                              error:(NSError *)error {
  // TODO: Emit characteristic update to Rust
}

// --- CBPeripheralManagerDelegate ---

- (void)peripheralManagerDidUpdateState:(CBPeripheralManager *)peripheral {
}

- (void)peripheralManagerDidStartAdvertising:(CBPeripheralManager *)peripheral
                                       error:(NSError *)error {
  if (error) {
    NSLog(@"[Oxide] Failed to start advertising: %@", error);
  } else {
    NSLog(@"[Oxide] Started advertising");
  }
}

- (void)peripheralManager:(CBPeripheralManager *)peripheral
         willRestoreState:(NSDictionary<NSString *, id> *)dict {
  NSLog(@"[Oxide] CBPeripheralManager willRestoreState");
}

@end

// ===== Exports =====

uint8_t oxide_ble_is_supported(void) { return 1; }

void oxide_ble_init_with_restoration(const char *restore_id_cstr) {
  static dispatch_once_t onceToken;
  dispatch_once(&onceToken, ^{
    NSString *restoreId =
        restore_id_cstr ? [NSString stringWithUTF8String:restore_id_cstr]
                        : nil;
    s_ble_manager =
        [[OxBleManager alloc] initWithRestoreIdentifier:restoreId];
  });
}

void oxide_ble_init(void) { oxide_ble_init_with_restoration(NULL); }

uint8_t oxide_ble_powered_on(void) {
  return BleContext().central.state == CBManagerStatePoweredOn;
}

void oxide_ble_shutdown(void) {
  OxBleManager *ctx = BleContext();
  if (ctx.central.isScanning) {
    [ctx.central stopScan];
  }
  [ctx.peripheralManager stopAdvertising];
}

void oxide_ble_start_scan(const OxBleScanConfig *cfg) {
  OxBleManager *ctx = BleContext();
  if (ctx.central.state != CBManagerStatePoweredOn) {
    return;
  }
  NSArray<CBUUID *> *services = BleServicesFromConfig(cfg);
  NSDictionary *opts = @{
    CBCentralManagerScanOptionAllowDuplicatesKey : @(cfg->allow_duplicates)
  };
  [ctx.central scanForPeripheralsWithServices:services options:opts];
}

void oxide_ble_stop_scan(void) { [BleContext().central stopScan]; }

void oxide_ble_connect(const uint8_t *addr, size_t addr_len) {
  if (addr_len != 16)
    return;
  NSUUID *uuid = [[NSUUID alloc] initWithUUIDBytes:addr];
  OxBleManager *ctx = BleContext();
  CBPeripheral *p = ctx.peripherals[uuid];
  if (p) {
    [ctx.central connectPeripheral:p options:nil];
  } else {
    NSArray *known = [ctx.central retrievePeripheralsWithIdentifiers:@[ uuid ]];
    if (known.firstObject) {
      ctx.peripherals[uuid] = known.firstObject;
      [ctx.central connectPeripheral:known.firstObject options:nil];
    }
  }
}

void oxide_ble_disconnect(const uint8_t *addr, size_t addr_len) {
  if (addr_len != 16)
    return;
  NSUUID *uuid = [[NSUUID alloc] initWithUUIDBytes:addr];
  OxBleManager *ctx = BleContext();
  CBPeripheral *p = ctx.peripherals[uuid];
  if (p) {
    [ctx.central cancelPeripheralConnection:p];
  }
}

// --- Advertising ---

void oxide_ble_advertise_start(const char *name,
                                 const uint8_t *service_uuid_bytes) {
  OxBleManager *ctx = BleContext();
  if (ctx.peripheralManager.state != CBManagerStatePoweredOn) {
    NSLog(@"[Oxide] Cannot start advertising: Bluetooth not powered on");
    return;
  }

  NSMutableDictionary *data = [NSMutableDictionary dictionary];

  if (name) {
    data[CBAdvertisementDataLocalNameKey] =
        [NSString stringWithUTF8String:name];
  }

  if (service_uuid_bytes) {
    NSData *uuidData = [NSData dataWithBytes:service_uuid_bytes length:16];
    CBUUID *uuid = [CBUUID UUIDWithData:uuidData];
    data[CBAdvertisementDataServiceUUIDsKey] = @[ uuid ];
  }

  [ctx.peripheralManager startAdvertising:data];
}

void oxide_ble_advertise_stop(void) {
  [BleContext().peripheralManager stopAdvertising];
}

// --- Stubs for read/write/subscribe (to be implemented fully if needed) ---

int oxide_ble_read_char(const uint8_t *addr, size_t addr_len,
                          const uint16_t *uuid16) {
  return 0;
}

int oxide_ble_write_char(const uint8_t *addr, size_t addr_len,
                           const uint16_t *uuid16, const uint8_t *data,
                           size_t len) {
  return 0;
}

int oxide_ble_subscribe(const uint8_t *addr, size_t addr_len,
                          const uint16_t *uuid16, uint8_t on) {
  return 0;
}
