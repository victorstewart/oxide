import React, { useEffect, useRef } from 'react';
import {
  ActivityIndicator,
  NativeModules,
  StatusBar,
  StyleSheet,
  Text,
  View,
} from 'react-native';
import {
  Camera,
  useCameraDevice,
  useCameraFormat,
  useCameraPermission,
} from 'react-native-vision-camera';

const BENCHMARK_FPS = 30;
const BENCHMARK_RESOLUTION = { width: 1280, height: 720 };
const BENCHMARK_PIXEL_FORMAT = 'yuv' as const;

type BenchmarkBridgeType = {
  logContract?: (contract: Record<string, unknown>) => void;
  logMarker?: (marker: string) => void;
};

function getBenchmarkBridge(): BenchmarkBridgeType {
  return (
    (NativeModules.BenchmarkBridge as BenchmarkBridgeType | undefined) ?? {}
  );
}

function App() {
  const requestedPermissionRef = useRef(false);
  const loggedContractRef = useRef(false);
  const { hasPermission, requestPermission } = useCameraPermission();
  const preferredDevice = useCameraDevice('back', {
    physicalDevices: ['wide-angle-camera'],
  });
  const fallbackDevice = useCameraDevice('back');
  const device = preferredDevice ?? fallbackDevice;
  const format = useCameraFormat(device, [
    { videoResolution: BENCHMARK_RESOLUTION },
    { fps: BENCHMARK_FPS },
  ]);

  useEffect(() => {
    if (hasPermission || requestedPermissionRef.current) {
      return;
    }
    requestedPermissionRef.current = true;
    void requestPermission();
  }, [hasPermission, requestPermission]);

  useEffect(() => {
    if (device == null || format == null || loggedContractRef.current) {
      return;
    }
    loggedContractRef.current = true;
    const negotiatedFps = Math.min(BENCHMARK_FPS, format.maxFps);
    getBenchmarkBridge().logContract?.({
      source: 'react-native-vision-camera',
      transport: 'native-preview-view',
      devicePosition: device.position,
      sessionPreset: `format:${format.videoWidth}x${format.videoHeight}`,
      requestedPixelFormat: BENCHMARK_PIXEL_FORMAT,
      activePixelFormat: BENCHMARK_PIXEL_FORMAT,
      requestedFps: BENCHMARK_FPS,
      requestedWidth: BENCHMARK_RESOLUTION.width,
      requestedHeight: BENCHMARK_RESOLUTION.height,
      activeWidth: format.videoWidth,
      activeHeight: format.videoHeight,
      activeFps: negotiatedFps,
      videoRange: 'unknown',
      colorSpace: 'unknown',
      wideColorAuto: false,
      mirrored: false,
      benchmark: 'react-native-vision-camera-preview',
      deviceId: device.id,
      deviceName: device.name,
      physicalDevices: device.physicalDevices,
      isMultiCam: device.isMultiCam,
      negotiatedMinFps: format.minFps,
      negotiatedMaxFps: format.maxFps,
      previewEnabled: true,
      photoEnabled: false,
      videoEnabled: false,
      audioEnabled: false,
      lowLightBoost: false,
      videoHdr: false,
      videoStabilizationMode: 'off',
      enableBufferCompression: true,
    });
  }, [device, format]);

  if (!hasPermission) {
    return <LoadingState label="Requesting camera permission..." />;
  }

  if (device == null || format == null) {
    return <LoadingState label="Waiting for camera device..." />;
  }

  return (
    <View style={styles.container}>
      <StatusBar hidden />
      <Camera
        audio={false}
        device={device}
        enableBufferCompression={true}
        fps={BENCHMARK_FPS}
        format={format}
        isActive={true}
        lowLightBoost={false}
        onInitialized={() => {
          getBenchmarkBridge().logMarker?.('OXIDE_READY');
        }}
        photo={false}
        pixelFormat={BENCHMARK_PIXEL_FORMAT}
        preview={true}
        resizeMode="cover"
        style={StyleSheet.absoluteFill}
        testID="camera-preview"
        video={false}
        videoHdr={false}
        videoStabilizationMode="off"
      />
    </View>
  );
}

function LoadingState({ label }: { label: string }) {
  return (
    <View style={styles.loadingContainer} testID="loading-state">
      <ActivityIndicator color="#ffffff" size="large" />
      <Text style={styles.loadingLabel}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000000',
  },
  loadingContainer: {
    flex: 1,
    backgroundColor: '#000000',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 16,
  },
  loadingLabel: {
    color: '#ffffff',
    fontSize: 16,
  },
});

export default App;
