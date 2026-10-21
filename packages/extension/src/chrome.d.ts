/**
 * The few `chrome.*` extension APIs the popup uses, typed by hand from Chrome's extension
 * reference so the package needs no type dependency.
 */
declare namespace chrome {
  namespace tabs {
    interface Tab {
      id?: number;
      url?: string;
    }
    function query(query: {
      active?: boolean;
      currentWindow?: boolean;
      url?: string;
    }): Promise<Tab[]>;
    function reload(tabId: number): Promise<void>;
  }

  namespace permissions {
    function request(permissions: { origins: string[] }): Promise<boolean>;
    function remove(permissions: { origins: string[] }): Promise<boolean>;
  }

  namespace scripting {
    interface RegisteredContentScript {
      id: string;
      matches?: string[];
      js?: string[];
      runAt?: "document_start" | "document_end" | "document_idle";
      world?: "MAIN" | "ISOLATED";
      allFrames?: boolean;
      persistAcrossSessions?: boolean;
    }
    function registerContentScripts(scripts: RegisteredContentScript[]): Promise<void>;
    function unregisterContentScripts(filter: { ids: string[] }): Promise<void>;
    function getRegisteredContentScripts(filter?: {
      ids?: string[];
    }): Promise<RegisteredContentScript[]>;
    function executeScript<Args extends unknown[]>(injection: {
      target: { tabId: number };
      world?: "MAIN" | "ISOLATED";
      func: (...args: Args) => unknown;
      args: Args;
    }): Promise<unknown[]>;
  }

  namespace storage {
    const local: {
      get(keys: string | string[]): Promise<Record<string, unknown>>;
      set(items: Record<string, unknown>): Promise<void>;
      remove(keys: string | string[]): Promise<void>;
    };
  }
}
