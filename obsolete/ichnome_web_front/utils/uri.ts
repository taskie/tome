export function mapTemplate<T>(f: (x: T) => string): (strings: TemplateStringsArray, ...values: T[]) => string {
  return (strings: TemplateStringsArray, ...values: T[]) => {
    return strings.reduce((result, string, i) => {
      return result + f(values[i - 1]) + string;
    });
  };
}

export const embedURIComponent = mapTemplate(encodeURIComponent);

const encodeURIComponentArrayAllowed = (uriComponent: string | string[] | number | boolean) => {
  if (typeof uriComponent === "string" || typeof uriComponent === "number" || typeof uriComponent === "boolean") {
    return encodeURIComponent(uriComponent);
  } else {
    return encodeURIComponent(uriComponent.join(","));
  }
};

export const embedURIComponentArrayAllowed = mapTemplate(encodeURIComponentArrayAllowed);

export const uri = embedURIComponent;

export const uria = embedURIComponentArrayAllowed;
