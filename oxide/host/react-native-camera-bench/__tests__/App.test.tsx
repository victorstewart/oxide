import React from 'react';
import { NativeModules } from 'react-native';
import ReactTestRenderer from 'react-test-renderer';
import { useCameraDevice, useCameraFormat, useCameraPermission } from 'react-native-vision-camera';
import App from '../App';

jest.mock('react-native-vision-camera', () => {
  const React = require('react');
  const { View } = require('react-native');
  return {
    Camera: (props: object) => React.createElement(View, props),
    useCameraDevice: jest.fn(),
    useCameraFormat: jest.fn(),
    useCameraPermission: jest.fn(),
  };
});

const mockedUseCameraDevice = jest.mocked(useCameraDevice);
const mockedUseCameraFormat = jest.mocked(useCameraFormat);
const mockedUseCameraPermission = jest.mocked(useCameraPermission);

describe('App', () => {
  beforeEach(() => {
    NativeModules.BenchmarkBridge = {
      logContract: jest.fn(),
      logMarker: jest.fn(),
    };
    mockedUseCameraDevice.mockReset();
    mockedUseCameraFormat.mockReset();
    mockedUseCameraPermission.mockReset();
  });

  test('renders the normalized preview contract', async () => {
    mockedUseCameraPermission.mockReturnValue({
      hasPermission: true,
      requestPermission: jest.fn(),
    });
    mockedUseCameraDevice
      .mockReturnValueOnce({
        id: 'preferred',
        name: 'Wide Camera',
        position: 'back',
        physicalDevices: ['wide-angle-camera'],
        isMultiCam: false,
      } as never)
      .mockReturnValueOnce({
        id: 'fallback',
        name: 'Fallback Camera',
        position: 'back',
        physicalDevices: ['wide-angle-camera'],
        isMultiCam: false,
      } as never);
    mockedUseCameraFormat.mockReturnValue({
      videoWidth: 1280,
      videoHeight: 720,
      minFps: 30,
      maxFps: 30,
    } as never);

    let tree: ReactTestRenderer.ReactTestRenderer;
    await ReactTestRenderer.act(() => {
      tree = ReactTestRenderer.create(<App />);
    });

    const preview = tree!.root.findByProps({ testID: 'camera-preview' });
    expect(preview.props.pixelFormat).toBe('yuv');
    expect(preview.props.fps).toBe(30);
    expect(preview.props.photo).toBe(false);
    expect(preview.props.video).toBe(false);
    expect(preview.props.audio).toBe(false);
    expect(preview.props.enableBufferCompression).toBe(true);
    expect(preview.props.lowLightBoost).toBe(false);
    expect(NativeModules.BenchmarkBridge.logContract).toHaveBeenCalledWith(
      expect.objectContaining({
        source: 'react-native-vision-camera',
        transport: 'native-preview-view',
        benchmark: 'react-native-vision-camera-preview',
        requestedPixelFormat: 'yuv',
        requestedFps: 30,
        requestedWidth: 1280,
        requestedHeight: 720,
        activePixelFormat: 'yuv',
        activeWidth: 1280,
        activeHeight: 720,
        activeFps: 30,
      }),
    );
  });

  test('renders a loading state until camera permission exists', async () => {
    const requestPermission = jest.fn().mockResolvedValue(false);
    mockedUseCameraPermission.mockReturnValue({
      hasPermission: false,
      requestPermission,
    });
    mockedUseCameraDevice.mockReturnValue(null as never);
    mockedUseCameraFormat.mockReturnValue(undefined);

    let tree: ReactTestRenderer.ReactTestRenderer;
    await ReactTestRenderer.act(() => {
      tree = ReactTestRenderer.create(<App />);
    });

    expect(tree!.root.findByProps({ testID: 'loading-state' })).toBeTruthy();
    expect(requestPermission).toHaveBeenCalledTimes(1);
  });
});
