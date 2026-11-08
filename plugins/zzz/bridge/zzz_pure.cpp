// zzz_pure.cpp - 纯逻辑单元的实现：g_py 全局与 WinRT JSON 解析。
// 不含 socket、进程、CPython 加载等宿主接口；测试目标与生产 DLL 共同链接。

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.Collections.h>

#include "zzz_pure.hpp"

PythonApi g_py;
PostCallFn g_postCall = nullptr;

namespace {

std::string Narrow(const winrt::hstring& text) {
    return winrt::to_string(text);
}

}  // namespace

bool JsonToJVar(const winrt::Windows::Data::Json::IJsonValue& value, JVar& out) {
    namespace W = winrt::Windows::Data::Json;
    if (!value) return false;
    switch (value.ValueType()) {
        case W::JsonValueType::Null:
            out = JVar{JVar::T::Null};
            return true;
        case W::JsonValueType::Boolean:
            out = JVar{JVar::T::Bool, value.GetBoolean()};
            return true;
        case W::JsonValueType::Number: {
            double number = value.GetNumber();
            if (std::isfinite(number) && number == std::floor(number) &&
                number >= -9007199254740992.0 && number <= 9007199254740992.0) {
                out = JVar{JVar::T::Int, false, static_cast<long long>(number)};
            } else {
                out = JVar{JVar::T::Real};
                out.d = number;
            }
            return true;
        }
        case W::JsonValueType::String:
            out = JVar{JVar::T::Str, false, 0, 0, Narrow(value.GetString())};
            return true;
        case W::JsonValueType::Array: {
            out = JVar{JVar::T::Arr};
            for (const auto& item : value.GetArray()) {
                JVar element;
                if (!JsonToJVar(item, element)) return false;
                out.arr.push_back(std::move(element));
            }
            return true;
        }
        case W::JsonValueType::Object: {
            out = JVar{JVar::T::Obj};
            for (const auto& [key, item] : value.GetObject()) {
                JVar element;
                if (!JsonToJVar(item, element)) return false;
                out.obj.emplace_back(Narrow(key), std::move(element));
            }
            return true;
        }
    }
    return false;
}
