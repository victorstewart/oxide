#import <Foundation/Foundation.h>
#include <stdint.h>

// This isolated 2D benchmark host has no camera acquisition session.
void oxide_cam_start_default(void) {}
void oxide_cam_stop(void) {}
void oxide_cam_release_acquired(uint32_t slot, uint64_t generation) {}

int32_t oxide_cam_get_latest_bgra(void **texture, int32_t *width, int32_t *height)
{
   if (texture) *texture = NULL;
   if (width) *width = 0;
   if (height) *height = 0;
   return 0;
}

int32_t oxide_cam_get_latest_ex(void **y, void **uv, int32_t *width, int32_t *height, int32_t *depth, int32_t *matrix, int32_t *range, int32_t *space)
{
   if (uv) *uv = NULL;
   if (depth) *depth = 0;
   if (matrix) *matrix = 0;
   if (range) *range = 0;
   if (space) *space = 0;
   return oxide_cam_get_latest_bgra(y, width, height);
}

void oxide_host_ios_log(const char *message, size_t length)
{
   if (!NSProcessInfo.processInfo.environment[@"OXIDE_CORE_DIAGNOSTICS"] || !message) return;
   NSString *line = [[NSString alloc] initWithBytes:message length:length encoding:NSUTF8StringEncoding];
   NSURL *root = [NSFileManager.defaultManager URLsForDirectory:NSDocumentDirectory inDomains:NSUserDomainMask].firstObject;
   NSURL *url = [root URLByAppendingPathComponent:@"core-diagnostics.log"];
   if (![NSFileManager.defaultManager fileExistsAtPath:url.path]) [NSData.data writeToURL:url atomically:YES];
   NSFileHandle *file = [NSFileHandle fileHandleForWritingToURL:url error:nil];
   [file seekToEndOfFile];
   [file writeData:[[line stringByAppendingString:@"\n"] dataUsingEncoding:NSUTF8StringEncoding]];
   [file closeFile];
}

int32_t oxide_host_power_lowpower(void)
{
   return NSProcessInfo.processInfo.lowPowerModeEnabled ? 1 : 0;
}

int32_t oxide_host_thermal_state(void)
{
   return (int32_t)NSProcessInfo.processInfo.thermalState;
}
