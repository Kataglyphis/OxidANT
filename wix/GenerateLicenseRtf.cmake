# Reuse the shared encoder so the installed RTF cannot drift from the project license.
cmake_minimum_required(VERSION 3.20)
if(NOT DEFINED LICENSE_INPUT OR NOT DEFINED LICENSE_OUTPUT)
  message(FATAL_ERROR "LICENSE_INPUT and LICENSE_OUTPUT are required")
endif()
include("${CMAKE_CURRENT_LIST_DIR}/../third_party/ANTfrastructure/cmake/CPackCommon.cmake")
kataglyphis_write_license_rtf("${LICENSE_INPUT}" "${LICENSE_OUTPUT}")
