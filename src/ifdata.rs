#![allow(clippy::all)] // don't care about mesages in generated code

use a2lfile::a2ml_specification;

a2ml_specification! {
    <A2mlVector>

    struct Protocol_Layer {
        uint protocol_version;  /// XCP protocol layer version, current 0x100
        uint t1;  /// T1 [ms]
        uint t2;  /// T2 [ms]
        uint t3;  /// T3 [ms]
        uint t4;  /// T4 [ms]
        uint t5;  /// T5 [ms]
        uint t6;  /// T6 [ms]
        uint t7;  /// T7 [ms]
        uchar max_cto;  /// MAX_CTO
        uint max_dto;  /// MAX_DTO
        enum {
            "BYTE_ORDER_MSB_LAST" = 0,
            "BYTE_ORDER_MSB_FIRST" = 1
        };
        enum {
            "ADDRESS_GRANULARITY_BYTE" = 1,
            "ADDRESS_GRANULARITY_WORD" = 2,
            "ADDRESS_GRANULARITY_DWORD" = 4
        };
        taggedstruct {
            ("OPTIONAL_CMD" enum XcpOptions {
                "GET_COMM_MODE_INFO" = 251,
                "GET_ID" = 250,
                "SET_REQUEST" = 249,
                "GET_SEED" = 248,
                "UNLOCK" = 247,
                "SET_MTA" = 246,
                "UPLOAD" = 245,
                "SHORT_UPLOAD" = 244,
                "BUILD_CHECKSUM" = 243,
                "TRANSPORT_LAYER_CMD" = 242,
                "USER_CMD" = 241,
                "DOWNLOAD" = 240,
                "DOWNLOAD_NEXT" = 239,
                "DOWNLOAD_MAX" = 238,
                "SHORT_DOWNLOAD" = 237,
                "MODIFY_BITS" = 236,
                "SET_CAL_PAGE" = 235,
                "GET_CAL_PAGE" = 234,
                "GET_PAG_PROCESSOR_INFO" = 233,
                "GET_SEGMENT_INFO" = 232,
                "GET_PAGE_INFO" = 231,
                "SET_SEGMENT_MODE" = 230,
                "GET_SEGMENT_MODE" = 229,
                "COPY_CAL_PAGE" = 228,
                "CLEAR_DAQ_LIST" = 227,
                "SET_DAQ_PTR" = 226,
                "WRITE_DAQ" = 225,
                "SET_DAQ_LIST_MODE" = 224,
                "GET_DAQ_LIST_MODE" = 223,
                "START_STOP_DAQ_LIST" = 222,
                "START_STOP_SYNCH" = 221,
                "GET_DAQ_CLOCK" = 220,
                "READ_DAQ" = 219,
                "GET_DAQ_PROCESSOR_INFO" = 218,
                "GET_DAQ_RESOLUTION_INFO" = 217,
                "GET_DAQ_LIST_INFO" = 216,
                "GET_DAQ_EVENT_INFO" = 215,
                "FREE_DAQ" = 214,
                "ALLOC_DAQ" = 213,
                "ALLOC_ODT" = 212,
                "ALLOC_ODT_ENTRY" = 211,
                "PROGRAM_START" = 210,
                "PROGRAM_CLEAR" = 209,
                "PROGRAM" = 208,
                "PROGRAM_RESET" = 207,
                "GET_PGM_PROCESSOR_INFO" = 206,
                "GET_SECTOR_INFO" = 205,
                "PROGRAM_PREPARE" = 204,
                "PROGRAM_FORMAT" = 203,
                "PROGRAM_NEXT" = 202,
                "PROGRAM_MAX" = 201,
                "PROGRAM_VERIFY" = 200,
                "WRITE_DAQ_MULTIPLE" = 199
            })*;
            "COMMUNICATION_MODE_SUPPORTED" taggedunion {
                "BLOCK" taggedstruct {
                    "SLAVE" ;
                    "MASTER" struct {
                        uchar max_bs;  /// MAX_BS
                        uchar min_st;  /// MIN_ST
                    };
                };
                "INTERLEAVED" uchar;  /// QUEUE_SIZE 
            };
            "SEED_AND_KEY_EXTERNAL_FUNCTION" char funcname[256];  /// Name of the Seed&Key function
            "MAX_DTO_STIM" uint max_dto_stim;  /// overrules MAX_DTO see above for STIM use case
        };
    };

    struct Daq {
        enum {
            "STATIC" = 0,
            "DYNAMIC" = 1
        };
        uint max_daq;            /// MAX_DAQ
        uint max_event_channel;  /// MAX_EVENT_CHANNEL
        uchar min_daq;           /// MIN_DAQ
        enum OptimisationType {
            "OPTIMISATION_TYPE_DEFAULT" = 0,
            "OPTIMISATION_TYPE_ODT_TYPE_16" = 1,
            "OPTIMISATION_TYPE_ODT_TYPE_32" = 2,
            "OPTIMISATION_TYPE_ODT_TYPE_64" = 3,
            "OPTIMISATION_TYPE_ODT_TYPE_ALIGNMENT" = 4,
            "OPTIMISATION_TYPE_MAX_ENTRY_SIZE" = 5
        };
        enum AddressExtension {
            "ADDRESS_EXTENSION_FREE" = 0,
            "ADDRESS_EXTENSION_ODT" = 1,
            "ADDRESS_EXTENSION_DAQ" = 3
        };
        enum IdentificationFieldType {
            "IDENTIFICATION_FIELD_TYPE_ABSOLUTE" = 0,
            "IDENTIFICATION_FIELD_TYPE_RELATIVE_BYTE" = 1,
            "IDENTIFICATION_FIELD_TYPE_RELATIVE_WORD" = 2,
            "IDENTIFICATION_FIELD_TYPE_RELATIVE_WORD_ALIGNED" = 3
        };
        enum GranularityOdtEntrySizeDaq {
            "GRANULARITY_ODT_ENTRY_SIZE_DAQ_BYTE" = 1,
            "GRANULARITY_ODT_ENTRY_SIZE_DAQ_WORD" = 2,
            "GRANULARITY_ODT_ENTRY_SIZE_DAQ_DWORD" = 4,
            "GRANULARITY_ODT_ENTRY_SIZE_DAQ_DLONG" = 8
        };
        uchar max_odt_entry_size_daq;  /// MAX_ODT_ENTRY_SIZE_DAQ
        enum OverloadIndication {
            "NO_OVERLOAD_INDICATION" = 0,
            "OVERLOAD_INDICATION_PID" = 1,
            "OVERLOAD_INDICATION_EVENT" = 2
        };
        taggedstruct {
            "DAQ_ALTERNATING_SUPPORTED" uint;  ///This flag selects the alternating display mode.
            "PRESCALER_SUPPORTED" ;
            "RESUME_SUPPORTED" ;
            "STORE_DAQ_SUPPORTED" ;  ///This flag indicates that the slave can store DAQ configurations.
            block "STIM" struct {
                enum GranularityOdtEntrySizeStim {
                    "GRANULARITY_ODT_ENTRY_SIZE_STIM_BYTE" = 1,
                    "GRANULARITY_ODT_ENTRY_SIZE_STIM_WORD" = 2,
                    "GRANULARITY_ODT_ENTRY_SIZE_STIM_DWORD" = 4,
                    "GRANULARITY_ODT_ENTRY_SIZE_STIM_DLONG" = 8
                };
                uchar max_odt_entry_size_stim;  /// MAX_ODT_ENTRY_SIZE_STIM
                taggedstruct {
                    "BIT_STIM_SUPPORTED" ;
                    "MIN_ST_STIM" uchar;  ///Separation time between DTOs time in units of 100 microseconds
                };
            };
            block "TIMESTAMP_SUPPORTED" struct {
                uint timestamp_ticks;  /// TIMESTAMP_TICKS
                enum TimestampSize {
                    "NO_TIME_STAMP" = 0,
                    "SIZE_BYTE" = 1,
                    "SIZE_WORD" = 2,
                    "SIZE_DWORD" = 4
                };
                enum TimestampUnit {
                    "UNIT_1NS" = 0,
                    "UNIT_10NS" = 1,
                    "UNIT_100NS" = 2,
                    "UNIT_1US" = 3,
                    "UNIT_10US" = 4,
                    "UNIT_100US" = 5,
                    "UNIT_1MS" = 6,
                    "UNIT_10MS" = 7,
                    "UNIT_100MS" = 8,
                    "UNIT_1S" = 9,
                    "UNIT_1PS" = 10,
                    "UNIT_10PS" = 11,
                    "UNIT_100PS" = 12
                };
                taggedstruct {
                    "TIMESTAMP_FIXED" ;
                };
            };
            "PID_OFF_SUPPORTED" ;
            "MAX_DAQ_TOTAL" uint;
            "MAX_ODT_TOTAL" uint;
            "MAX_ODT_DAQ_TOTAL" uint;
            "MAX_ODT_STIM_TOTAL" uint;
            "MAX_ODT_ENTRIES_TOTAL" uint;
            "MAX_ODT_ENTRIES_DAQ_TOTAL" uint;
            "MAX_ODT_ENTRIES_STIM_TOTAL" uint;
            "CPU_LOAD_MAX_TOTAL" float;
            block "DAQ_MEMORY_CONSUMPTION" struct {
                ulong daq_memory_limit;  /// "DAQ_MEMORY_LIMIT"
                uint daq_size;  /// "DAQ_SIZE" : Bytes pro DAQ-Liste
                uint odt_size;  /// "ODT_SIZE" : Bytes pro ODT
                uint odt_entry_size;  /// "ODT_ENTRY_SIZE" : Bytes pro ODT_Entry
                uint odt_daq_buffer_factor;  /// "ODT_DAQ_BUFFER_FACTOR"  : Nutzbytes * Faktor = Bytes für Sendepuffer
                uint odt_stim_buffer_factor;  /// "ODT_STIM_BUFFER_FACTOR" : Nutzbytes * Faktor = Bytes für Empfangspuffer
            };
            (block "DAQ_LIST" struct {
                uint daq_list_number;  /// DAQ_LIST_NUMBER
                taggedstruct {
                    "DAQ_LIST_TYPE" enum {
                        "DAQ" = 1,
                        "STIM" = 2,
                        "DAQ_STIM" = 3
                    };
                    "MAX_ODT" uchar;
                    "MAX_ODT_ENTRIES" uchar;
                    "FIRST_PID" uchar;
                    "EVENT_FIXED" uint;
                    block "PREDEFINED" taggedstruct {
                        (block "ODT" struct {
                            uchar odt_number;  /// ODT number
                            taggedstruct {
                                ("ODT_ENTRY" struct {
                                    uchar odt_entry_number;  /// ODT_ENTRY number
                                    ulong element_address;  /// address of element
                                    uchar address_extension;  /// address extension of element
                                    uchar element_size;  /// size of element [AG]
                                    uchar bit_offset;  /// BIT_OFFSET
                                })*;
                            };
                        })*;
                    };
                };
            })*;
            (block "EVENT" struct {
                char event_channel_name[101];  /// EVENT_CHANNEL_NAME
                char event_channel_short_name[9];  /// EVENT_CHANNEL_SHORT_NAME
                uint event_channel_number;  /// EVENT_CHANNEL_NUMBER
                enum {
                    "DAQ" = 1,
                    "STIM" = 2,
                    "DAQ_STIM" = 3
                };
                uchar max_daq_list;  /// MAX_DAQ_LIST
                uchar time_cycle;  /// TIME_CYCLE
                uchar time_unit;  /// TIME_UNIT
                uchar priority;  /// PRIORITY
                taggedstruct {
                    "COMPLEMENTARY_BYPASS_EVENT_CHANNEL_NUMBER" uint;  ///This keyword is used to make a combination of two event channels building a bypassing raster.
                    "CONSISTENCY" enum {
                        "DAQ" = 0,
                        "EVENT" = 1
                    };  ///With this keyword, the slave can indicate what kind of data consistency exists when data are processed within this Event.
                    block "MIN_CYCLE_TIME" struct {
                        uchar event_channel_time_cycle;  /// EVENT_CHANNEL_TIME_CYCLE
                        uchar event_channel_time_unit;  /// EVENT_CHANNEL_TIME_UNIT
                    };
                    "CPU_LOAD_MAX" float;
                    block "CPU_LOAD_CONSUMPTION_DAQ" struct {
                        float daq_factor;  /// "DAQ_FACTOR"
                        float odt_factor;  /// "ODT_FACTOR"
                        float odt_entry_factor;  /// "ODT_ENTRY_FACTOR" 
                        taggedstruct {
                            (block "ODT_ENTRY_SIZE_FACTOR_TABLE" struct {
                                uint size;  /// "SIZE"
                                float size_factor;  /// "SIZE_FACTOR"
                            })*;
                        };
                    };
                    block "CPU_LOAD_CONSUMPTION_STIM" struct {
                        float;  /// "DAQ_FACTOR"
                        float;  /// "ODT_FACTOR"
                        float;  /// "ODT_ENTRY_FACTOR"
                        taggedstruct {
                            (block "ODT_ENTRY_SIZE_FACTOR_TABLE" struct {
                                uint size;  /// "SIZE"
                                float size_factor;  /// "SIZE_FACTOR"
                            })*;
                        };
                    };
                    block "CPU_LOAD_CONSUMPTION_QUEUE" struct {
                        float odt_factor;  /// "ODT_FACTOR"
                        float odt_length_factor;  /// "ODT_LENGTH_FACTOR", length in elements[AG]
                    };
                };
            })*;
        };
    };

    taggedunion Daq_Event {
        "FIXED_EVENT_LIST" taggedstruct {
            ("EVENT" uint)*;
        };
        "VARIABLE" taggedstruct {
            block "AVAILABLE_EVENT_LIST" taggedstruct {
                ("EVENT" uint)*;
            };
            block "DEFAULT_EVENT_LIST" taggedstruct {
                ("EVENT" uint)*;
            };
        };
    };

    struct Pag {
        uchar max_segments;  /// MAX_SEGMENTS
        taggedstruct {
            "FREEZE_SUPPORTED" ;
        };
    };

    struct Pgm {
        enum PgmMode {
            "PGM_MODE_ABSOLUTE" = 1,
            "PGM_MODE_FUNCTIONAL" = 2,
            "PGM_MODE_ABSOLUTE_AND_FUNCTIONAL" = 3
        };
        uchar max_sectors;  /// MAX_SECTORS
        uchar max_cto_pgm;  /// MAX_CTO_PGM
        taggedstruct {
            (block "SECTOR" struct {
                char sector_name[101];  /// SECTOR_NAME
                uchar sector_number;  /// SECTOR_NUMBER
                ulong address;  /// Address
                ulong length;  /// Length
                uchar clear_sequence_number;  /// CLEAR_SEQUENCE_NUMBER
                uchar program_sequence_number;  /// PROGRAM_SEQUENCE_NUMBER
                uchar program_method;  /// PROGRAM_METHOD
            })*;
            "COMMUNICATION_MODE_SUPPORTED" taggedunion {
                "BLOCK" taggedstruct {
                    "SLAVE" ;
                    "MASTER" struct {
                        uchar max_bs;  /// MAX_BS_PGM
                        uchar min_st;  /// MIN_ST_PGM
                    };
                };
                "INTERLEAVED" uchar;  /// QUEUE_SIZE_PGM
            };
        };
    };

    struct Segment {
        uchar segment_number;  /// SEGMENT_NUMBER
        uchar num_pages;  /// number of pages
        uchar address_extension;  /// ADDRESS_EXTENSION
        uchar compression_method;  /// COMPRESSION_METHOD
        uchar encryption_method;  /// ENCRYPTION_METHOD
        taggedstruct {
            block "CHECKSUM" struct {
                enum XcpChecksumType {
                    "XCP_ADD_11" = 1,
                    "XCP_ADD_12" = 2,
                    "XCP_ADD_14" = 3,
                    "XCP_ADD_22" = 4,
                    "XCP_ADD_24" = 5,
                    "XCP_ADD_44" = 6,
                    "XCP_CRC_16" = 7,
                    "XCP_CRC_16_CITT" = 8,
                    "XCP_CRC_32" = 9,
                    "XCP_USER_DEFINED" = 255
                };
                taggedstruct {
                    "MAX_BLOCK_SIZE" ulong max_block_size;
                    "EXTERNAL_FUNCTION" char dllname[256];  /// Name of the Checksum.DLL
                };
            };
            (block "PAGE" struct {
                uchar page_number;  /// PAGE_NUMBER
                enum EcuAccessPermission {
                    "ECU_ACCESS_NOT_ALLOWED" = 0,
                    "ECU_ACCESS_WITHOUT_XCP_ONLY" = 1,
                    "ECU_ACCESS_WITH_XCP_ONLY" = 2,
                    "ECU_ACCESS_DONT_CARE" = 3
                };
                enum XcpReadAccessPermission {
                    "XCP_READ_ACCESS_NOT_ALLOWED" = 0,
                    "XCP_READ_ACCESS_WITHOUT_ECU_ONLY" = 1,
                    "XCP_READ_ACCESS_WITH_ECU_ONLY" = 2,
                    "XCP_READ_ACCESS_DONT_CARE" = 3
                };
                enum XcpWriteAccessPermission {
                    "XCP_WRITE_ACCESS_NOT_ALLOWED" = 0,
                    "XCP_WRITE_ACCESS_WITHOUT_ECU_ONLY" = 1,
                    "XCP_WRITE_ACCESS_WITH_ECU_ONLY" = 2,
                    "XCP_WRITE_ACCESS_DONT_CARE" = 3
                };
                taggedstruct {
                    "INIT_SEGMENT" uchar;  /// references segment that initialises this page
                };
            })*;
            (block "ADDRESS_MAPPING" struct {
                ulong source_address;  /// source address
                ulong dest_address;  /// destination address
                ulong length;  /// length
            })*;
            "PGM_VERIFY" ulong;  /// verification value for PGM
        };
    };

    taggedstruct Common_Parameters {
        block "PROTOCOL_LAYER" struct Protocol_Layer;
        block "SEGMENT" struct Segment;
        block "DAQ" struct Daq;
        block "PAG" struct Pag;
        block "PGM" struct Pgm;
        block "DAQ_EVENT" taggedunion Daq_Event;
    };

    struct CAN_Parameters {
        uint;  /// XCP on CAN version, currently 0x0100
        taggedstruct {
            "CAN_ID_BROADCAST" ulong value;  /// Auto-detection CAN-ID
            "CAN_ID_MASTER" ulong value;  /// CMD/STIM CAN-ID
            "CAN_ID_MASTER_INCREMENTAL" ;  /// Master uses range of CAN-IDs. Start of range = CAN_ID_MASTER
            "CAN_ID_SLAVE" ulong value;  /// RES/ERR/EV/SERV/DAQ CAN-ID
            "BAUDRATE" ulong value;  /// Baudrate in Hz
            "SAMPLE_POINT" uchar value;  /// Sample point in % of bit time
            "SAMPLE_RATE" enum {
                "SINGLE" = 1,
                "TRIPLE" = 3
            };
            "BTL_CYCLES" uchar value;  /// slots per bit time
            "SJW" uchar value; /// sync jump width
            "SYNC_EDGE" enum {
                "SINGLE" = 1,
                "DUAL" = 2
            };
            "MAX_DLC_REQUIRED" ;  /// master to slave frames
            (block "DAQ_LIST_CAN_ID" struct {
                uint daq_list_ref;  /// reference to DAQ_LIST_NUMBER
                taggedstruct {
                    "VARIABLE" ;
                    "FIXED" ulong id;  /// this DAQ_LIST always on this CAN_ID
                };
            })*;
            (block "EVENT_CAN_ID_LIST" struct {
                uint event_ref;  /// reference to EVENT_NUMBER
                taggedstruct {
                    ("FIXED" ulong id)*;  /// this Event always on this IDs
                };
            })*;
            "MAX_BUS_LOAD" ulong;  /// maximum available bus in bit/s
            block "CAN_FD" struct {
                taggedstruct {
                    "MAX_DLC" uint value;  /// 8, 12, 16, 20, 24, 32, 48 or 64
                    "CAN_FD_DATA_TRANSFER_BAUDRATE" ulong value;  /// BAUDRATE [Hz]
                    "SAMPLE_POINT" uchar value;  /// sample point receiver [% complete bit time]
                    "BTL_CYCLES" uchar value;  /// BTL_CYCLES [slots per bit time]
                    "SJW" uchar value;  /// length synchr. segment [BTL_CYCLES]
                    "SYNC_EDGE" enum {
                        "SINGLE" = 1,
                        "DUAL" = 2
                    };
                    "MAX_DLC_REQUIRED" ;  /// master to slave frames always to have DLC = MAX_DLC_for CAN-FD
                    "SECONDARY_SAMPLE_POINT" uchar value;  /// sender sample point [% complete bit time]
                    "TRANSCEIVER_DELAY_COMPENSATION" enum {
                        "OFF" = 0,
                        "ON" = 1
                    };
                };
            };
        };
    };

    struct SxI_Parameters {
        uint xcp_on_sxi_version;  /// XCP on SxI version, currently 0x0100
        ulong baudrate;  /// BAUDRATE [Hz] 
        taggedstruct {
            "ASYNCH_FULL_DUPLEX_MODE" struct {
                enum Parity {
                    "PARITY_NONE" = 0,
                    "PARITY_ODD" = 1,
                    "PARITY_EVEN" = 2
                };
                enum StopBits {
                    "ONE_STOP_BIT" = 1,
                    "TWO_STOP_BITS" = 2
                };
                taggedstruct {
                    block "FRAMING" struct {
                        uchar;  /// SYNC character
                        uchar;  /// ESC character
                    };
                };  /// Support for framing mechanism
            };
            "SYNCH_FULL_DUPLEX_MODE_BYTE" ;
            "SYNCH_FULL_DUPLEX_MODE_WORD" ;
            "SYNCH_FULL_DUPLEX_MODE_DWORD" ;
            "SYNCH_MASTER_SLAVE_MODE_BYTE" ;
            "SYNCH_MASTER_SLAVE_MODE_WORD" ;
            "SYNCH_MASTER_SLAVE_MODE_DWORD" ;
        };
        enum HeaderLen {
            "HEADER_LEN_BYTE" = 0,
            "HEADER_LEN_CTR_BYTE" = 1,
            "HEADER_LEN_FILL_BYTE" = 2,
            "HEADER_LEN_WORD" = 3,
            "HEADER_LEN_CTR_WORD" = 4,
            "HEADER_LEN_FILL_WORD" = 5
        };
        enum SxiChecksum {
            "NO_CHECKSUM" = 0,
            "CHECKSUM_BYTE" = 1,
            "CHECKSUM_WORD" = 2
        };
    };

    struct TCP_IP_Parameters {
        uint version;  /// XCP on TCP_IP version, currently 0x0100
        uint port;  /// PORT
        taggedunion {
            "HOST_NAME" char hostname[256];
            "ADDRESS" char address_v4[15];
            "IPV6" char address_v6[39];
        };
        taggedstruct {
            "MAX_BUS_LOAD" ulong;  /// maximum available bus load in percent
            "MAX_BIT_RATE" ulong;  /// Network speed which is the base for MAX_BUS_LOAD in Mbit
        };
    };

    struct UDP_IP_Parameters {
        uint version;  /// XCP on UDP version, currently 0x0100
        uint port;  /// PORT
        taggedunion {
            "HOST_NAME" char hostname[256];
            "ADDRESS" char address_v4[15];
            "IPV6" char address_v6[39];
        };
        taggedstruct {
            "MAX_BUS_LOAD" ulong;  /// maximum available bus load in percent
            "MAX_BIT_RATE" ulong;  /// Network speed which is the base for MAX_BUS_LOAD in Mbit
        };
    };

    struct ep_parameters {
        uchar endpoint_number;  /// ENDPOINT_NUMBER, not endpoint address
        enum {
            "BULK_TRANSFER" = 2,
            "INTERRUPT_TRANSFER" = 3
        };
        uint wMaxPacketSize;  /// wMaxPacketSize: Maximum packet size of endpoint in bytes
        uchar bInterval;  /// bInterval: polling of endpoint
        enum {
            "MESSAGE_PACKING_SINGLE" = 0,
            "MESSAGE_PACKING_MULTIPLE" = 1,
            "MESSAGE_PACKING_STREAMING" = 2
        };
        enum {
            "ALIGNMENT_8_BIT" = 0,
            "ALIGNMENT_16_BIT" = 1,
            "ALIGNMENT_32_BIT" = 2,
            "ALIGNMENT_64_BIT" = 3
        };
        taggedstruct {
            "RECOMMENDED_HOST_BUFSIZE" uint;  /// Recommended size for the host buffer size. The size is defined as multiple of wMaxPacketSize.  
        };
    };  /// end of ep_parameters

    struct USB_Parameters {
        uint version;  /// XCP on USB version e.g. "1.0" = 0x0100
        uint vendor_id;  /// Vendor ID
        uint product_id;  /// Product ID
        uchar interface;  /// Number of interface
        enum {
            "HEADER_LEN_BYTE" = 0,
            "HEADER_LEN_CTR_BYTE" = 1,
            "HEADER_LEN_FILL_BYTE" = 2,
            "HEADER_LEN_WORD" = 3,
            "HEADER_LEN_CTR_WORD" = 4,
            "HEADER_LEN_FILL_WORD" = 5
        };
        taggedunion {
            block "OUT_EP_CMD_STIM" struct ep_parameters;
        };
        taggedunion {
            block "IN_EP_RESERR_DAQ_EVSERV" struct ep_parameters;
        };
        taggedstruct {
            "ALTERNATE_SETTING_NO" uchar;  /// Number of alternate setting
            "INTERFACE_STRING_DESCRIPTOR" char[101];
            (block "OUT_EP_ONLY_STIM" struct ep_parameters)*;
            (block "IN_EP_ONLY_DAQ" struct ep_parameters)*;
            block "IN_EP_ONLY_EVSERV" struct ep_parameters;
            (block "DAQ_LIST_USB_ENDPOINT" struct {
                uint;  /// reference to DAQ_LIST_NUMBER
                taggedstruct {
                    "FIXED_IN" uchar;  /// this DAQ list always ENDPOINT_NUMBER, not endpoint address
                    "FIXED_OUT" uchar;  /// this STIM list always ENDPOINT_NUMBER, not endpoint address
                };
            })*;  /// end of DAQ_LIST_USB_ENDPOINT
        };  /// end of optional
    };

    enum PacketAssignmentType {
        "NOT_ALLOWED" = 0,
        "FIXED" = 1,
        "VARIABLE_INITIALISED" = 2,
        "VARIABLE" = 3
    };  /// end of PacketAssignmentType

    struct Buffer {
        uchar flx_buf;  /// FLX_BUF
        taggedstruct {
            "MAX_FLX_LEN_BUF" taggedunion {
                "FIXED" uchar length;  /// constant value
                "VARIABLE" uchar length;  /// initial value
            };  /// end of MAX_FLX_LEN_BUF
            block "LPDU_ID" taggedstruct {
                "FLX_SLOT_ID" taggedunion {
                    "FIXED" uint slot_id;
                    "VARIABLE" taggedstruct {
                        "INITIAL_VALUE" uint slot_id;
                    };
                };  /// end of FLX_SLOT_ID
                "OFFSET" taggedunion {
                    "FIXED" uchar offset;
                    "VARIABLE" taggedstruct {
                        "INITIAL_VALUE" uchar offset;
                    };
                };  /// end of OFFSET
                "CYCLE_REPETITION" taggedunion {
                    "FIXED" uchar cycle;
                    "VARIABLE" taggedstruct {
                        "INITIAL_VALUE" uchar cycle;
                    };
                };  /// end of CYCLE_REPETITION
                "CHANNEL" taggedunion {
                    "FIXED" enum {
                        "A" = 0,
                        "B" = 1
                    } channel;
                    "VARIABLE" taggedstruct {
                        "INITIAL_VALUE" enum {
                            "A" = 0,
                            "B" = 1
                        } channel;
                    };
                };  /// end of CHANNEL
            };  /// end of LPDU_ID
            block "XCP_PACKET" taggedstruct {
                "CMD" enum PacketAssignmentType;  /// end of CMD
                "RES_ERR" enum PacketAssignmentType;  /// end of RES_ERR
                "EV_SERV" enum PacketAssignmentType;  /// end of EV_SERV
                "DAQ" enum PacketAssignmentType;  /// end of DAQ
                "STIM" enum PacketAssignmentType;  /// end of STIM
            };  /// end of XCP_PACKET
        };
    };  /// end of Buffer

    struct FLX_Parameters {
        uint version;  /// XCP on FlexRay version e.g. "1.0" = 0x0100
        uint t1;  /// T1_FLX [ms]
        char fibex_file[256];  /// FIBEX-file including CHI information including extension, without path
        char cluster_id[256];  /// Cluster-ID
        uchar nax;  /// NAX
        enum {
            "HEADER_NAX" = 0,
            "HEADER_NAX_FILL" = 1,
            "HEADER_NAX_CTR" = 2,
            "HEADER_NAX_FILL3" = 3,
            "HEADER_NAX_CTR_FILL2" = 4,
            "HEADER_NAX_LEN" = 5,
            "HEADER_NAX_CTR_LEN" = 6,
            "HEADER_NAX_FILL2_LEN" = 7,
            "HEADER_NAX_CTR_FILL_LEN" = 8
        };
        enum {
            "PACKET_ALIGNMENT_8" = 0,
            "PACKET_ALIGNMENT_16" = 1,
            "PACKET_ALIGNMENT_32" = 2
        };
        taggedunion {
            block "INITIAL_CMD_BUFFER" struct Buffer;
        };
        taggedunion {
            block "INITIAL_RES_ERR_BUFFER" struct Buffer;
        };
        taggedstruct {
            (block "POOL_BUFFER" struct Buffer)*;
        };
    };

    block "IF_DATA" taggedunion if_data {

        "CANAPE_EXT" struct {
            int;             /// version number
            taggedstruct {
                "LINK_MAP" struct {
                    char symbol_name[256];   /// symbol name
                    long address;            /// base address of the segment
                    uint address_ext;        /// address extension of the segment
                    uint ds_relative;        /// flag: address is relative to DS
                    long segment_offset;     /// offset of the segment address
                    uint datatype_valid;     /// datatypValid
                    uint datatype;           /// enum datatyp
                    uint bit_offset;         /// bit offset of the data (bitfield)
                };
                "DISPLAY" struct {
                    long color;              /// display color
                    double display_min;      /// minimal display value (phys)
                    double display_max;      /// maximal display value (phys)
                };
                "VIRTUAL_CONVERSION" struct {
                    char conversion_name[256];   /// name of the conversion formula
                };
            };
        };
        "CANAPE_MODULE" struct {
            taggedstruct {
                ("RECORD_LAYOUT_STEPSIZE" struct {
                    char record_layout[256];   /// name of record layout
                    uint fnc_values_step;      /// stepsize for FNC_VALUES
                    uint axis_pts_x_step;      /// stepsize for AXIS_PTS_X
                    uint axis_pts_y_step;      /// stepsize for AXIS_PTS_Y
                    uint axis_pts_z_step;      /// stepsize for AXIS_PTS_Z
                    uint axis_pts_4_step;      /// stepsize for AXIS_PTS_4
                    uint axis_pts_5_step;      /// stepsize for AXIS_PTS_5
                })*;
            };
        };
        "CANAPE_ADDRESS_UPDATE" taggedstruct {
            ("EPK_ADDRESS" struct {
                char epk_sym[1024];         /// name of the corresponding symbol in MAP file
                long offset;               /// optional address offset
            })*;
            "ECU_CALIBRATION_OFFSET" struct {
                char sym[1024];         /// name of the corresponding symbol in MAP file
                long offset;               /// optional address offset
            };
            (block "CALIBRATION_METHOD" taggedunion {
                "AUTOSAR_SINGLE_POINTERED" struct {
                    char pointer_tbl[1024];         /// MAP symbol name for pointer table in RAM
                    long offset;               /// optional address offset
                    taggedstruct {
                        "ORIGINAL_POINTER_TABLE" struct {
                            char[1024];    /// MAP symbol name for pointer table in FLASH
                            long;          /// optional address offset
                        };
                    };
                };
                "InCircuit2" struct {
                    char[1024];         /// MAP symbol name for pointer table in RAM
                    long;               /// optional address offset
                    taggedstruct {
                        "ORIGINAL_POINTER_TABLE" struct {
                            char[1024];    /// MAP symbol name for pointer table in FLASH
                            long;          /// optional address offset
                        };
                        "FLASH_SECTION" struct {
                            ulong;       /// start address of flash section
                            ulong;       /// length of flash section
                        };
                    };
                };
            })*;
            block "MAP_SYMBOL" taggedstruct {
                "FIRST" struct {
                    char[1024];  /// symbol name of the corresponding segment in MAP file
                    long;        /// offset
                };
                "LAST" struct {
                    char[1024];  /// symbol name of the corresponding segment in MAP file
                    long;        /// offset
                };
                ("ADDRESS_MAPPING_XCP" struct {
                    char[1024];  /// symbol name of source range in MAP file
                    char[1024];  /// symbol name of destination range in MAP file
                })*;
            };
            (block "MEMORY_SEGMENT" struct {
                char[1024];         /// name of the memory segment
                taggedstruct {
                    "FIRST" struct {
                        char[1024];  /// symbol name of the corresponding segment in MAP file
                        long;        /// offset
                    };
                    "LAST" struct {
                        char[1024];  /// symbol name of the corresponding segment in MAP file
                        long;        /// offset
                    };
                    ("ADDRESS_MAPPING_XCP" struct {
                        char[1024];  /// symbol name of source range in MAP file
                        char[1024];  /// symbol name of destination range in MAP file
                    })*;
                };
            })*;
        };
        "CANAPE_CAL_METHOD" taggedstruct {
            (block "CAL_PARAM_GROUP" taggedstruct {
                "NAME" char name[1024];
                "ADDRESS" ulong address;
                "SIZE" ulong size;
                "COMMENT" char comment[1024];
                "LINK_MAP" struct {
                    char symbol_name[256];   /// symbol name
                    ulong base_address;      /// base address of the symbol
                    uint address_extension;  /// address extension of the symbol
                    uint rel_address;        /// flag: address is relative to DS
                    long symbol_offset;      /// offset of the symbol address
                };
            })*;
        };
        "CANAPE_GROUP" taggedstruct {
            block "STRUCTURE_LIST" (char name[1024])*;
        };

        "XCP" struct {
            taggedstruct Common_Parameters;  /// default parameters
            taggedstruct {
                block "XCP_ON_CAN" struct {
                    struct CAN_Parameters;  /// specific for CAN
                    taggedstruct Common_Parameters;  /// overruling of default
                };
                block "XCP_ON_SxI" struct {
                    struct SxI_Parameters;  /// specific for SxI
                    taggedstruct Common_Parameters;  /// overruling of default
                };
                block "XCP_ON_TCP_IP" struct {
                    struct TCP_IP_Parameters;  /// specific for TCP_IP
                    taggedstruct Common_Parameters;  /// overruling of default
                };
                block "XCP_ON_UDP_IP" struct {
                    struct UDP_IP_Parameters;  /// specific for UDP
                    taggedstruct Common_Parameters;  /// overruling of default
                };
                block "XCP_ON_USB" struct {
                    struct USB_Parameters;  /// specific for USB
                    taggedstruct Common_Parameters;  /// overruling of default
                };
                block "XCP_ON_FLX" struct {
                    struct FLX_Parameters;  /// specific for FlexRay
                    taggedstruct Common_Parameters;  /// overruling of default
                };
            };
        };

        "ASAP1B_CCP" taggedstruct {
            "DP_BLOB" struct {
                uint address_extension;  /// Address extension of the calibration data
                ulong base_address;  /// Base address of the calibration data
                ulong size;  /// Number of Bytes belonging to the calibration data
            };  /// address information for calibration objects and memory segments
        };
    };
}
