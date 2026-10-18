use std::ffi::c_void;

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    DEVICE_STATE_ACTIVE, DigitalAudioDisplayDevice, ERole, Headphones, Headset, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, PKEY_AudioEndpoint_FormFactor, eCommunications,
    eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantToBSTR, PropVariantToUInt32};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ};
use windows::core::{GUID, HRESULT, HSTRING, IUnknown, IUnknown_Vtbl, Interface, PCWSTR, PWSTR};
use winisland_platform::{AudioOutputDevice, AudioOutputKind, PlatformError};

use crate::com::ComGuard;

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
const IID_POLICY_CONFIG: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);

/// Leading slots of the undocumented `IPolicyConfig` interface Windows uses to switch outputs.
#[repr(C)]
struct PolicyConfigVtbl {
    base: IUnknown_Vtbl,
    unused: [usize; 10],
    set_default_endpoint: unsafe extern "system" fn(*mut c_void, PCWSTR, ERole) -> HRESULT,
}

fn take_string(value: PWSTR) -> String {
    // SAFETY: The string was allocated by COM for the caller and is freed exactly once.
    unsafe {
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0 as *const c_void));
        text
    }
}

fn device_id(device: &IMMDevice) -> Option<String> {
    // SAFETY: device is used on the COM thread that created it.
    unsafe { device.GetId().ok().map(take_string) }
}

fn describe(device: &IMMDevice, default_id: Option<&str>) -> Option<AudioOutputDevice> {
    let id = device_id(device)?;
    // SAFETY: The property store and its values stay on the COM thread that opened them.
    let (name, form_factor) = unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let name = store
            .GetValue(&PKEY_Device_FriendlyName)
            .and_then(|value| PropVariantToBSTR(&value))
            .map(|name| name.to_string())
            .unwrap_or_default();
        let form_factor = store
            .GetValue(&PKEY_AudioEndpoint_FormFactor)
            .and_then(|value| PropVariantToUInt32(&value))
            .map_or(-1, |value| value as i32);
        (name, form_factor)
    };
    let kind = if form_factor == Headphones.0 || form_factor == Headset.0 {
        AudioOutputKind::Headphones
    } else if form_factor == DigitalAudioDisplayDevice.0 {
        AudioOutputKind::Display
    } else {
        AudioOutputKind::Speakers
    };
    Some(AudioOutputDevice {
        is_default: default_id == Some(id.as_str()),
        id,
        name,
        kind,
    })
}

pub(super) fn output_devices() -> Result<Vec<AudioOutputDevice>, PlatformError> {
    let _com = ComGuard::mta()?;
    // SAFETY: The enumerator and collection are used on this COM-initialized thread.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(PlatformError::backend)?;
        let default_id = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .ok()
            .and_then(|device| device_id(&device));
        let collection = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(PlatformError::backend)?;
        let count = collection.GetCount().map_err(PlatformError::backend)?;
        Ok((0..count)
            .filter_map(|index| collection.Item(index).ok())
            .filter_map(|device| describe(&device, default_id.as_deref()))
            .collect())
    }
}

pub(super) fn set_default_output(id: &str) -> Result<(), PlatformError> {
    let _com = ComGuard::mta()?;
    let id = HSTRING::from(id);
    // SAFETY: The policy object is queried for its documented-by-convention IID and released
    // when `policy` drops; the vtable layout matches IPolicyConfig on Windows 10 and later.
    unsafe {
        let unknown: IUnknown = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)
            .map_err(PlatformError::backend)?;
        let mut raw = std::ptr::null_mut();
        unknown
            .query(&IID_POLICY_CONFIG, &mut raw)
            .ok()
            .map_err(PlatformError::backend)?;
        let policy = IUnknown::from_raw(raw);
        let vtable = *(policy.as_raw() as *const *const PolicyConfigVtbl);
        for role in [eConsole, eMultimedia, eCommunications] {
            ((*vtable).set_default_endpoint)(policy.as_raw(), PCWSTR(id.as_ptr()), role)
                .ok()
                .map_err(PlatformError::backend)?;
        }
    }
    Ok(())
}
